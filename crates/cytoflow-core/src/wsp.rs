//! FlowJo 10 `.wsp` workspace importer.
//!
//! The `.wsp` format is plain XML embedding Gating-ML-style gate elements.
//! This reader is written from the observable structure of workspace files
//! (and the public FlowKit/CytoML descriptions of it); it does not depend on
//! anything from the FlowJo program itself.
//!
//! Imported: sample list, per-channel transforms (`linear`, `log`, `logicle`,
//! `fasinh`, FlowJo `biex` approximated as logicle), the first spillover
//! matrix, and the population tree (rectangle/quadrant, polygon, ellipsoid
//! and boolean `And/Or/Not` nodes) of each sample.

use crate::compensation::CompMatrix;
use crate::error::{Error, Result};
use crate::gate::{BoolOp, GateDim, GateNode, GateShape};
use crate::transform::Transform;
use crate::workspace::{ChannelSetting, CompSource, Workspace};
use crate::xml::{self, Element};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedCount {
    pub sample_name: String,
    pub gate_path: String,
    pub count: usize,
}

#[derive(Debug)]
pub struct WspImport {
    pub workspace: Workspace,
    pub warnings: Vec<String>,
    /// Population counts recorded by FlowJo, useful for cross-checking.
    pub expected_counts: Vec<ExpectedCount>,
    /// Samples whose FCS files could not be found.
    pub missing_files: Vec<(String, PathBuf)>,
}

#[derive(Debug, Clone)]
struct WspSample {
    name: String,
    path: PathBuf,
    transforms: BTreeMap<String, Transform>,
    tree: Vec<GateNode>,
    counts: Vec<(String, usize)>,
}

/// FlowJo's biexponential (`width` basis, `pos` decades, `neg` decades) mapped
/// onto a logicle scale using the Parks et al. relation
/// `W = (M − log10(T / |width|)) / 2`.
pub fn biex_to_logicle(max_range: f64, width: f64, pos: f64, neg: f64) -> Transform {
    let t = if max_range > 0.0 { max_range } else { 262144.0 };
    let m = if pos > 0.0 { pos } else { 4.5 };
    let w = if width.abs() > 0.0 { ((m - (t / width.abs()).log10()) / 2.0).clamp(0.0, m / 2.0) } else { 0.5 };
    Transform::Logicle { t, w, m, a: neg.max(0.0) }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn uri_to_path(uri: &str) -> PathBuf {
    let mut s = uri.trim().to_string();
    if let Some(rest) = s.strip_prefix("file:") {
        s = rest.to_string();
    }
    s = percent_decode(&s);
    // `file:////server/share/x` or `file:/C:/x`
    let trimmed = s.trim_start_matches('/');
    if trimmed.len() > 1 && trimmed.as_bytes()[1] == b':' {
        // Windows drive letter
        return PathBuf::from(trimmed);
    }
    PathBuf::from(format!("/{trimmed}"))
}

/// Per-channel transforms of a sample, plus the coordinate scale of each
/// channel: FlowJo `linear` transforms carry a `gain` (e.g. for `Time`, the
/// acquisition time step), and both the transform range and the gate
/// coordinates on that channel are expressed in `raw · gain` units.
struct Transforms {
    transforms: BTreeMap<String, Transform>,
    coord_scale: BTreeMap<String, f64>,
}

fn parse_transforms(container: &Element, comp_prefixes: &[String]) -> Transforms {
    let mut out = BTreeMap::new();
    let mut coord_scale = BTreeMap::new();
    // Transforms of compensated (`Comp-`) parameters take precedence over the
    // uncompensated parameter of the same detector, since gates on
    // fluorescence channels are normally drawn on the compensated values.
    let mut from_comp: BTreeMap<String, bool> = BTreeMap::new();
    for t in &container.children {
        let Some(p) = t.child("parameter").and_then(|p| p.attr("name")) else { continue };
        let (name, is_comp) = strip_prefix(p, comp_prefixes);
        if !is_comp && from_comp.get(&name).copied().unwrap_or(false) {
            continue;
        }
        let tr = match t.local() {
            "linear" => {
                let gain = t.attr_f64("gain").filter(|g| *g > 0.0 && g.is_finite()).unwrap_or(1.0);
                if (gain - 1.0).abs() > 1e-12 {
                    coord_scale.insert(name.clone(), gain);
                } else {
                    coord_scale.remove(&name);
                }
                Transform::Linear { t: t.attr_f64("maxRange").unwrap_or(262144.0) / gain, a: -t.attr_f64("minRange").unwrap_or(0.0) / gain }
            }
            "log" => Transform::Log { t: 10f64.powf(t.attr_f64("offset").unwrap_or(0.0) + t.attr_f64("decades").unwrap_or(4.5)), m: t.attr_f64("decades").unwrap_or(4.5) },
            "logicle" => Transform::Logicle { t: t.attr_f64("T").unwrap_or(262144.0), w: t.attr_f64("W").unwrap_or(0.5), m: t.attr_f64("M").unwrap_or(4.5), a: t.attr_f64("A").unwrap_or(0.0) },
            "fasinh" => Transform::Asinh { t: t.attr_f64("T").unwrap_or(262144.0), m: t.attr_f64("M").unwrap_or(4.5), a: t.attr_f64("A").unwrap_or(0.0) },
            "biex" => biex_to_logicle(t.attr_f64("maxRange").unwrap_or(262144.0), t.attr_f64("width").unwrap_or(-10.0), t.attr_f64("pos").unwrap_or(4.5), t.attr_f64("neg").unwrap_or(0.0)),
            "hyperlog" => Transform::Logicle { t: t.attr_f64("T").unwrap_or(262144.0), w: t.attr_f64("W").unwrap_or(0.5), m: t.attr_f64("M").unwrap_or(4.5), a: t.attr_f64("A").unwrap_or(0.0) },
            _ => continue,
        };
        if t.local() != "linear" {
            coord_scale.remove(&name);
        }
        from_comp.insert(name.clone(), is_comp);
        out.insert(name, tr);
    }
    Transforms { transforms: out, coord_scale }
}

/// Strip a compensation prefix (`Comp-`) from a dimension name.
fn strip_prefix(name: &str, prefixes: &[String]) -> (String, bool) {
    for p in prefixes {
        if !p.is_empty() {
            if let Some(rest) = name.strip_prefix(p.as_str()) {
                return (rest.to_string(), true);
            }
        }
    }
    if let Some(rest) = name.strip_prefix("Comp-") {
        return (rest.to_string(), true);
    }
    (name.to_string(), false)
}

fn dim_name(e: &Element) -> Option<String> {
    e.child("fcs-dimension").and_then(|d| d.attr("name")).map(String::from)
}

struct CoordMap {
    transform: Transform,
    scale: f64,
}

impl CoordMap {
    fn apply(&self, v: f64) -> f64 {
        self.transform.apply(v / self.scale)
    }
}

struct TreeCtx<'a> {
    prefixes: &'a [String],
    transforms: &'a BTreeMap<String, Transform>,
    coord_scale: &'a BTreeMap<String, f64>,
    warnings: &'a mut Vec<String>,
    counts: Vec<(String, usize)>,
    next_id: usize,
    /// FlowJo gate id → our id, for boolean dependents by name we also keep a path map.
    path_to_id: HashMap<String, String>,
}

impl TreeCtx<'_> {
    /// The transform of a channel, composed with the channel's coordinate
    /// scale: maps a gate coordinate as written in the workspace to display space.
    fn transform(&self, ch: &str) -> CoordMap {
        CoordMap {
            transform: self.transforms.get(ch).cloned().unwrap_or(Transform::Linear { t: 262144.0, a: 0.0 }),
            scale: self.coord_scale.get(ch).copied().unwrap_or(1.0),
        }
    }

    /// Returns the shape, the `eventsInside=0` negation flag and the
    /// evaluation grid (polygons only, from `gateResolution`).
    fn convert_gate(&mut self, gate_el: &Element) -> Option<(GateShape, bool, Option<u32>)> {
        let shape_el = gate_el.children.iter().find(|c| c.local().ends_with("Gate"))?;
        let negate = shape_el.attr("eventsInside").is_some_and(|v| v == "0");
        let dims: Vec<(String, bool)> = shape_el.children_named("dimension").filter_map(|d| dim_name(d).map(|n| strip_prefix(&n, self.prefixes))).collect();
        let gd = |d: &(String, bool)| GateDim { channel: d.0.clone(), compensated: d.1 };
        match shape_el.local() {
            "RectangleGate" => {
                let dim_els: Vec<&Element> = shape_el.children_named("dimension").collect();
                if dim_els.len() != dims.len() || dims.is_empty() {
                    return None;
                }
                let mut min = Vec::new();
                let mut max = Vec::new();
                for (k, d) in dim_els.iter().enumerate() {
                    let tr = self.transform(&dims[k].0);
                    min.push(d.attr_f64("min").map(|v| tr.apply(v)));
                    max.push(d.attr_f64("max").map(|v| tr.apply(v)));
                }
                Some((GateShape::Rectangle { dims: dims.iter().map(gd).collect(), min, max }, negate, None))
            }
            "PolygonGate" => {
                if dims.len() != 2 {
                    return None;
                }
                let tx = self.transform(&dims[0].0);
                let ty = self.transform(&dims[1].0);
                let vertices: Vec<[f64; 2]> = shape_el
                    .children_named("vertex")
                    .filter_map(|v| {
                        let c: Vec<f64> = v.children_named("coordinate").filter_map(|x| x.attr_f64("value")).collect();
                        (c.len() == 2).then(|| [tx.apply(c[0]), ty.apply(c[1])])
                    })
                    .collect();
                // FlowJo tests polygon membership on events snapped to its
                // `gateResolution` display grid: with the snapping the counts of
                // linear-scale polygons are reproduced exactly.
                let grid = shape_el.attr_f64("gateResolution").filter(|r| *r >= 1.0).unwrap_or(256.0) as u32;
                Some((GateShape::Polygon { dims: [gd(&dims[0]), gd(&dims[1])], vertices }, negate, Some(grid)))
            }
            "EllipsoidGate" => {
                if dims.len() != 2 {
                    return None;
                }
                // FlowJo stores ellipsoid foci/edge vertices on its 0..256 display grid.
                let res = shape_el.attr_f64("gateResolution").unwrap_or(256.0);
                let pts = |name: &str| -> Vec<[f64; 2]> {
                    shape_el
                        .child(name)
                        .map(|p| {
                            p.children_named("vertex")
                                .filter_map(|v| {
                                    let c: Vec<f64> = v.children_named("coordinate").filter_map(|x| x.attr_f64("value")).collect();
                                    (c.len() == 2).then(|| [c[0] / res, c[1] / res])
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                };
                let foci = pts("foci");
                let edge = pts("edge");
                if foci.len() != 2 || edge.is_empty() {
                    self.warnings.push("ellipsoid gate without foci/edge skipped".into());
                    return None;
                }
                Some((GateShape::ellipse_from_foci([gd(&dims[0]), gd(&dims[1])], foci[0], foci[1], edge[0]), negate, None))
            }
            other => {
                self.warnings.push(format!("unsupported gate type {other} skipped"));
                None
            }
        }
    }

    fn walk(&mut self, subpops: &Element, parent: Option<&str>, parent_path: &str, out: &mut Vec<GateNode>) {
        for pop in &subpops.children {
            let name = pop.attr("name").unwrap_or("Population").to_string();
            let path = if parent_path.is_empty() { name.clone() } else { format!("{parent_path}/{name}") };
            let count = pop.attr("count").and_then(|c| c.parse::<usize>().ok());
            let shape: Option<(GateShape, bool, Option<u32>)> = match pop.local() {
                "Population" => pop.child("Gate").and_then(|g| self.convert_gate(g)),
                "AndNode" | "OrNode" | "NotNode" => {
                    let op = match pop.local() {
                        "AndNode" => BoolOp::And,
                        "OrNode" => BoolOp::Or,
                        _ => BoolOp::Not,
                    };
                    let mut refs = Vec::new();
                    if let Some(deps) = pop.child("Dependents") {
                        for d in deps.children_named("Dependent") {
                            let Some(n) = d.attr("name") else { continue };
                            // Dependents are given as paths relative to the sample root or as names.
                            let key = n.trim_start_matches('/').to_string();
                            let found = self.path_to_id.get(&key).cloned().or_else(|| {
                                self.path_to_id.iter().find(|(p, _)| p.ends_with(&format!("/{key}")) || **p == key).map(|(_, id)| id.clone())
                            });
                            match found {
                                Some(id) => refs.push(id),
                                None => self.warnings.push(format!("boolean node {name}: unknown dependent {n}")),
                            }
                        }
                    }
                    if refs.is_empty() { None } else { Some((GateShape::Boolean { op, refs }, false, None)) }
                }
                "Statistic" => continue,
                _ => None,
            };
            let Some((shape, negate, grid)) = shape else {
                if pop.local() == "Population" {
                    self.warnings.push(format!("population {path}: gate could not be imported"));
                }
                continue;
            };
            let id = format!("g{}", self.next_id);
            self.next_id += 1;
            self.path_to_id.insert(path.clone(), id.clone());
            if let Some(c) = count {
                self.counts.push((path.clone(), c));
            }
            out.push(GateNode { id: id.clone(), name, parent: parent.map(String::from), gate: crate::gate::Gate { shape, negate, grid }, color: None });
            if let Some(sub) = pop.child("Subpopulations") {
                self.walk(sub, Some(&id), &path, out);
            }
        }
    }
}

/// Parse a `.wsp` file. FCS files are searched next to the workspace (and one
/// directory level below) when the recorded absolute path does not exist.
pub fn import_wsp<P: AsRef<Path>>(path: P, search_dirs: &[PathBuf]) -> Result<WspImport> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)?;
    let root = xml::parse(&text)?;
    let mut warnings = Vec::new();
    if !root.is("Workspace") {
        return Err(Error::Workspace("not a FlowJo workspace (no <Workspace> root)".into()));
    }

    // Compensation matrices.
    let mut matrices: Vec<CompMatrix> = Vec::new();
    let mut prefixes: Vec<String> = Vec::new();
    for m in root.descendants("spilloverMatrix") {
        if let Some(p) = m.attr("prefix") {
            if !prefixes.contains(&p.to_string()) {
                prefixes.push(p.to_string());
            }
        }
        let params: Vec<String> = m.child("parameters").map(|p| p.children_named("parameter").filter_map(|x| x.attr("name").map(String::from)).collect()).unwrap_or_default();
        let mut rows = Vec::new();
        for s in m.children_named("spillover") {
            let mut row = vec![0.0; params.len()];
            for c in s.children_named("coefficient") {
                if let (Some(p), Some(v)) = (c.attr("parameter"), c.attr_f64("value")) {
                    if let Some(j) = params.iter().position(|x| x == p) {
                        row[j] = v;
                    }
                }
            }
            rows.push(row);
        }
        // The same matrix is usually repeated in `<Matrices>` and in every
        // `<Sample>`; keep distinct ones only.
        if !params.is_empty() && rows.len() == params.len() {
            let m = CompMatrix { channels: params, spillover: rows };
            if !matrices.contains(&m) {
                matrices.push(m);
            }
        }
    }
    if matrices.len() > 1 {
        warnings.push(format!("{} distinct compensation matrices found; using the first", matrices.len()));
    }

    // Samples.
    let mut samples: Vec<WspSample> = Vec::new();
    if let Some(list) = root.child("SampleList") {
        for s in list.children_named("Sample") {
            let uri = s.child("DataSet").and_then(|d| d.attr("uri")).unwrap_or_default();
            let node = s.child("SampleNode");
            let name = node.and_then(|n| n.attr("name")).map(String::from).unwrap_or_else(|| uri_to_path(uri).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default());
            let Transforms { transforms, coord_scale } = s
                .child("Transformations")
                .map(|t| parse_transforms(t, &prefixes))
                .unwrap_or(Transforms { transforms: BTreeMap::new(), coord_scale: BTreeMap::new() });
            let mut tree = Vec::new();
            let mut ctx = TreeCtx { prefixes: &prefixes, transforms: &transforms, coord_scale: &coord_scale, warnings: &mut warnings, counts: Vec::new(), next_id: 1, path_to_id: HashMap::new() };
            if let Some(sub) = node.and_then(|n| n.child("Subpopulations")) {
                ctx.walk(sub, None, "", &mut tree);
            }
            let counts = ctx.counts;
            samples.push(WspSample { name, path: uri_to_path(uri), transforms, tree, counts });
        }
    }
    if samples.is_empty() {
        warnings.push("workspace has no samples".into());
    }

    // Build the workspace: transforms first so that samples keep them.
    let ws_name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Imported".into());
    let mut ws = Workspace::new(&ws_name);
    let template = samples.iter().max_by_key(|s| s.tree.len()).cloned();
    if let Some(t) = &template {
        for (ch, tr) in &t.transforms {
            ws.channel_settings.insert(ch.clone(), ChannelSetting { transform: tr.clone() });
        }
    }
    for s in &samples {
        for (ch, tr) in &s.transforms {
            ws.channel_settings.entry(ch.clone()).or_insert(ChannelSetting { transform: tr.clone() });
        }
    }

    let mut missing = Vec::new();
    let mut expected_counts = Vec::new();
    let mut dirs: Vec<PathBuf> = search_dirs.to_vec();
    if let Some(d) = path.parent() {
        dirs.push(d.to_path_buf());
    }
    for s in &samples {
        let resolved = resolve_file(&s.path, &dirs);
        match resolved {
            Some(p) => {
                if let Err(e) = ws.add_sample(&p) {
                    warnings.push(format!("sample {}: {e}", s.name));
                }
            }
            None => missing.push((s.name.clone(), s.path.clone())),
        }
        for (gp, c) in &s.counts {
            expected_counts.push(ExpectedCount { sample_name: s.name.clone(), gate_path: gp.clone(), count: *c });
        }
    }
    if let Some(m) = matrices.into_iter().next() {
        ws.set_compensation(CompSource::Custom { matrix: m });
    } else {
        ws.set_compensation(CompSource::None);
    }
    if let Some(t) = template {
        ws.gates = t.tree;
        let differing = samples.iter().filter(|s| s.tree.len() != ws.gates.len()).count();
        if differing > 0 {
            warnings.push(format!("{differing} sample(s) have a different gate tree; the largest tree was applied to all samples"));
        }
    }
    Ok(WspImport { workspace: ws, warnings, expected_counts, missing_files: missing })
}

fn resolve_file(p: &Path, dirs: &[PathBuf]) -> Option<PathBuf> {
    if p.exists() {
        return Some(p.to_path_buf());
    }
    let name = p.file_name()?;
    for d in dirs {
        let c = d.join(name);
        if c.exists() {
            return Some(c);
        }
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let sub = e.path();
                if sub.is_dir() {
                    let c = sub.join(name);
                    if c.exists() {
                        return Some(c);
                    }
                }
            }
        }
    }
    None
}
