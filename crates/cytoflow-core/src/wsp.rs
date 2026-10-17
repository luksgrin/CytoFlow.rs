//! FlowJo 10 `.wsp` workspace importer.
//!
//! The `.wsp` format is plain XML embedding Gating-ML-style gate elements.
//! This reader is written from the observable structure of workspace files
//! (and the public FlowKit/CytoML descriptions of it); it does not depend on
//! anything from the FlowJo program itself.
//!
//! Imported: sample list, per-channel transforms (`linear`, `log`, `logicle`,
//! `fasinh`, FlowJo `biex`), the first spillover
//! matrix, the population tree (rectangle/quadrant, polygon, ellipsoid
//! and boolean `And/Or/Not` nodes) of each sample, and the sample groups with
//! their template trees.

use crate::compensation::CompMatrix;
use crate::error::{Error, Result};
use crate::gate::{BoolOp, GateDim, GateNode, GateShape};
use crate::transform::Transform;
use crate::workspace::{ChannelSetting, CompSource, SampleGate, Workspace, ALL_SAMPLES_NAME};
use crate::xml::{self, Element};
use std::collections::{BTreeMap, HashMap, HashSet};
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

/// Logicle approximation of a FlowJo biexponential (`width` basis, `pos`
/// decades, `neg` decades), kept as a fallback helper: the import itself
/// uses the exact [`Transform::Biex`]. The biex scale is the logicle family
/// with `M = pos`, `A = neg` and `W = log10(|width|) / 2` (the
/// correspondence CytoML uses to map FlowJo logicle onto its biex,
/// `widthBasis = −10^{2W}`); the two differ only by FlowJo's table
/// quantisation and normalisation (≤ 10⁻³ of the display range).
pub fn biex_to_logicle(max_range: f64, width: f64, pos: f64, neg: f64) -> Transform {
    let t = if max_range > 0.0 { max_range } else { 262144.0 };
    let m = if pos > 0.0 { pos } else { 4.5 };
    let w = (width.abs().max(1.0).log10() / 2.0).clamp(0.0, m / 2.0);
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
            // FlowJo's own biexponential, with the file's parameters.
            "biex" => Transform::Biex { t: t.attr_f64("maxRange").filter(|v| *v > 0.0).unwrap_or(262144.0), w: t.attr_f64("width").unwrap_or(-10.0), pos: t.attr_f64("pos").unwrap_or(4.5), neg: t.attr_f64("neg").unwrap_or(0.0) },
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

/// FlowJo's display grid (cells per axis) used by the edge-of-axis rule of
/// imported rectangle gates; see the `"RectangleGate"` arm of `convert_gate`.
const RECT_EDGE_GRID: f64 = 256.0;

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
                // FlowJo edge-of-axis rule (FlowJo import only; rectangles
                // drawn in the app or read from Gating-ML keep their exact
                // bounds): membership is tested on the exact display values
                // with the half-open `min ≤ x < max`, except that a bound
                // lying within the first (last) cell of the 256-step display
                // grid (or beyond the axis), i.e. a gate drawn to the edge of
                // the axis, is open. Evidence (tests/wsp_rect_edge.rs): the
                // E01 `Time` gate's minimum sits 0.02 cells above the axis
                // minimum and FlowJo counts all events, including the 2 below
                // it; in E05 the same minimum sits 1.035 cells up and FlowJo
                // excludes the 357 events below it (17 of them in the same
                // cell). Evaluating rectangles on the grid instead
                // (events and/or bounds snapped by round/floor/ceil) breaks
                // the exact Time, quadrant and biex-rectangle counts by tens
                // to thousands of events.
                let edge = 1.0 / RECT_EDGE_GRID;
                let mut min = Vec::new();
                let mut max = Vec::new();
                for (k, d) in dim_els.iter().enumerate() {
                    let tr = self.transform(&dims[k].0);
                    min.push(d.attr_f64("min").map(|v| tr.apply(v)).filter(|v| *v >= edge));
                    max.push(d.attr_f64("max").map(|v| tr.apply(v)).filter(|v| *v <= 1.0 - edge));
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
                // FlowJo's ellipse, as observed in the public fixtures:
                // * foci and edge vertices are in display space scaled to a
                //   0..`gateResolution` (default 256) grid, i.e. `display·256`;
                // * the four edge vertices are the ends of the major axis
                //   (first two) and of the minor axis (last two); the foci are
                //   derived from them (`c² = a² − b²` holds to rounding), so
                //   the ellipse is: centre = midpoint of the foci, semi-major
                //   axis `a` = half the summed focal distances of the first
                //   edge vertex, `b = √(a² − c²)`;
                // * `gating:distance` is *not* `2a` (on the 51-event FlowKit
                //   fixture it would admit 66 events instead of FlowJo's 51; it
                //   appears to be measured in screen pixels of the plot window);
                // * membership is tested on the exact display values: on linear
                //   axes this reproduces FlowJo exactly (tests/wsp_ellipse.rs),
                //   whereas snapping events to the grid (as for polygons) or
                //   flooring them to bins does not (flooring merges events 50
                //   and 51 of that fixture).
                // On logicle axes the same geometry undercounts FlowJo by 2-3 %
                // (8_color_ICS_with_ellipse.wsp). Ruled out as explanations
                // (tests/wsp_ellipse_explore.rs prints the counts): FlowJo's
                // biex in place of the logicle (CytoML mapping, 4096-, 1024-
                // or 256-channel tables), rounding/flooring the events,
                // a bin-aligned logicle zero, a grid spanning the data range,
                // polygonal ellipses. The deficit behaves like an offset of
                // the logicle grid by ≈ −1.2 units (or, equivalently here,
                // M ≈ 4.397 / W ≈ 1.025), which nothing in the workspace
                // explains, so it is not applied; about 10-15 events of it
                // come from the parent polygons. See tests/wsp_import.rs.
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

/// Gate trees are walked per sample with sample-local ids; this map gives the
/// same population path the same workspace id in every sample and group, so
/// that a gate edited through a group finds its copy in every member.
#[derive(Default)]
struct IdMap {
    by_path: HashMap<String, String>,
    next: usize,
}

impl IdMap {
    fn id_for(&mut self, path: &str) -> String {
        if let Some(id) = self.by_path.get(path) {
            return id.clone();
        }
        self.next += 1;
        let id = format!("g{}", self.next);
        self.by_path.insert(path.to_string(), id.clone());
        id
    }

    /// Rewrite the local ids of a walked tree (`local`: path → local id) to workspace ids.
    fn relabel(&mut self, tree: &mut [GateNode], local: &HashMap<String, String>) {
        let mut map: HashMap<String, String> = HashMap::new();
        // Assign new ids in tree order so that ids follow the population order.
        for n in tree.iter() {
            if let Some((path, _)) = local.iter().find(|(_, lid)| **lid == n.id) {
                map.insert(n.id.clone(), self.id_for(path));
            }
        }
        let get = |id: &str| map.get(id).cloned().unwrap_or_else(|| id.to_string());
        for n in tree.iter_mut() {
            n.id = get(&n.id);
            n.parent = n.parent.as_deref().map(get);
            if let GateShape::Boolean { refs, .. } = &mut n.gate.shape {
                for r in refs.iter_mut() {
                    *r = get(r);
                }
            }
        }
    }
}

/// A walked tree (local ids), its path → local id map and the FlowJo counts.
type Walked = (Vec<GateNode>, HashMap<String, String>, Vec<(String, usize)>);

/// Walk a `<Subpopulations>` element with the given transforms.
fn walk_tree(subpops: Option<&Element>, prefixes: &[String], tr: &Transforms, warnings: &mut Vec<String>) -> Walked {
    let mut tree = Vec::new();
    let mut ctx = TreeCtx { prefixes, transforms: &tr.transforms, coord_scale: &tr.coord_scale, warnings, counts: Vec::new(), next_id: 1, path_to_id: HashMap::new() };
    if let Some(sub) = subpops {
        ctx.walk(sub, None, "", &mut tree);
    }
    (tree, ctx.path_to_id, ctx.counts)
}

/// Parse a `.wsp` file. FCS files are searched next to the workspace (and one
/// directory level below) when the recorded absolute path does not exist.
///
/// Each sample keeps its own gate tree and its own transforms (stored as
/// per-sample overrides of the workspace defaults, which are taken from the
/// sample with the largest tree). `<Groups>` become groups whose
/// `<Subpopulations>` are the templates; a sample's gate is marked `tailored`
/// when a group of that sample has the same population with a different
/// gate. Samples whose FCS file is missing are still added (not loaded) so
/// that their trees are kept.
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

    // Samples: transforms first, so that the defaults are known before the
    // trees are converted.
    struct RawSample<'a> {
        wsp_id: Option<String>,
        name: String,
        path: PathBuf,
        tr: Transforms,
        subpops: Option<&'a Element>,
        size: usize,
    }
    let mut raws: Vec<RawSample> = Vec::new();
    if let Some(list) = root.child("SampleList") {
        for s in list.children_named("Sample") {
            let ds = s.child("DataSet");
            let uri = ds.and_then(|d| d.attr("uri")).unwrap_or_default();
            let node = s.child("SampleNode");
            let name = node.and_then(|n| n.attr("name")).map(String::from).unwrap_or_else(|| uri_to_path(uri).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default());
            let wsp_id = node.and_then(|n| n.attr("sampleID")).or_else(|| ds.and_then(|d| d.attr("sampleID"))).map(String::from);
            let tr = s.child("Transformations").map(|t| parse_transforms(t, &prefixes)).unwrap_or(Transforms { transforms: BTreeMap::new(), coord_scale: BTreeMap::new() });
            let subpops = node.and_then(|n| n.child("Subpopulations"));
            let size = subpops.map(|e| e.descendants("Population").len()).unwrap_or(0);
            raws.push(RawSample { wsp_id, name, path: uri_to_path(uri), tr, subpops, size });
        }
    }
    if raws.is_empty() {
        warnings.push("workspace has no samples".into());
    }

    // Workspace defaults: the transforms of the (first) sample with the
    // largest tree, completed by the other samples'.
    let template = raws.iter().enumerate().max_by_key(|(i, r)| (r.size, std::cmp::Reverse(*i))).map(|(i, _)| i);
    let mut defaults: BTreeMap<String, Transform> = BTreeMap::new();
    let mut default_scale: BTreeMap<String, f64> = BTreeMap::new();
    if let Some(t) = template {
        defaults = raws[t].tr.transforms.clone();
        default_scale = raws[t].tr.coord_scale.clone();
    }
    for r in &raws {
        for (ch, t) in &r.tr.transforms {
            defaults.entry(ch.clone()).or_insert_with(|| t.clone());
        }
    }
    // A sample converts its gates with its own transforms, falling back to the defaults.
    let effective = |r: &RawSample| -> Transforms {
        let mut t = defaults.clone();
        t.extend(r.tr.transforms.iter().map(|(k, v)| (k.clone(), v.clone())));
        Transforms { transforms: t, coord_scale: r.tr.coord_scale.clone() }
    };
    let default_tr = Transforms { transforms: defaults.clone(), coord_scale: default_scale };

    let mut ids = IdMap::default();
    let mut trees: Vec<Vec<GateNode>> = Vec::new();
    let mut counts: Vec<Vec<(String, usize)>> = Vec::new();
    for r in &raws {
        let (mut tree, local, c) = walk_tree(r.subpops, &prefixes, &effective(r), &mut warnings);
        ids.relabel(&mut tree, &local);
        trees.push(tree);
        counts.push(c);
    }

    // Groups (templates in the default transform space).
    struct RawGroup<'a> {
        name: String,
        members: Vec<String>,
        subpops: Option<&'a Element>,
        tree: Vec<GateNode>,
    }
    let mut groups: Vec<RawGroup> = Vec::new();
    if let Some(gs) = root.child("Groups") {
        for g in gs.children_named("GroupNode") {
            let name = g.attr("name").or_else(|| g.child("Group").and_then(|x| x.attr("name"))).unwrap_or("Group").to_string();
            let members: Vec<String> = g
                .child("Group")
                .and_then(|x| x.child("SampleRefs"))
                .map(|refs| refs.children_named("SampleRef").filter_map(|r| r.attr("sampleID").map(String::from)).collect())
                .unwrap_or_default();
            let subpops = g.child("Subpopulations");
            let (mut tree, local, _) = walk_tree(subpops, &prefixes, &default_tr, &mut warnings);
            ids.relabel(&mut tree, &local);
            groups.push(RawGroup { name, members, subpops, tree });
        }
    }

    // Tailored: the sample's gate differs from the same population of the
    // template of one of its groups (converted with the sample's transforms).
    let mut tailored: Vec<HashSet<String>> = vec![HashSet::new(); raws.len()];
    for (k, r) in raws.iter().enumerate() {
        let eff = effective(r);
        for g in &groups {
            let member = g.name == ALL_SAMPLES_NAME || r.wsp_id.as_ref().is_some_and(|id| g.members.contains(id));
            if !member || g.tree.is_empty() {
                continue;
            }
            let mut quiet = Vec::new();
            let (mut t, local, _) = walk_tree(g.subpops, &prefixes, &eff, &mut quiet);
            ids.relabel(&mut t, &local);
            for n in &trees[k] {
                if let Some(tn) = t.iter().find(|x| x.id == n.id) {
                    if tn.gate != n.gate || tn.name != n.name {
                        tailored[k].insert(n.id.clone());
                    }
                }
            }
        }
    }

    // Build the workspace.
    let ws_name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Imported".into());
    let mut ws = Workspace::new(&ws_name);
    for (ch, tr) in &defaults {
        ws.channel_settings.insert(ch.clone(), ChannelSetting { transform: tr.clone() });
    }
    // Channels used by gates but without a FlowJo transform were converted
    // with the linear fallback; make that the default so the gates keep their meaning.
    for n in trees.iter().flatten().chain(groups.iter().flat_map(|g| g.tree.iter())) {
        for d in n.gate.shape.dims() {
            ws.channel_settings.entry(d.channel.clone()).or_insert(ChannelSetting { transform: Transform::Linear { t: 262144.0, a: 0.0 } });
        }
    }

    let mut missing = Vec::new();
    let mut expected_counts = Vec::new();
    let mut dirs: Vec<PathBuf> = search_dirs.to_vec();
    if let Some(d) = path.parent() {
        dirs.push(d.to_path_buf());
    }
    let mut wsp_to_sid: HashMap<String, String> = HashMap::new();
    for (k, r) in raws.iter().enumerate() {
        for (gp, c) in &counts[k] {
            expected_counts.push(ExpectedCount { sample_name: r.name.clone(), gate_path: gp.clone(), count: *c });
        }
        let sid = match resolve_file(&r.path, &dirs) {
            Some(p) => match ws.add_sample(&p) {
                Ok(id) => id,
                Err(e) => {
                    warnings.push(format!("sample {}: {e}", r.name));
                    continue;
                }
            },
            None => {
                missing.push((r.name.clone(), r.path.clone()));
                ws.add_unloaded_sample(&r.name, &r.path)
            }
        };
        if let Some(w) = &r.wsp_id {
            wsp_to_sid.insert(w.clone(), sid.clone());
        }
        if let Some(sample) = ws.samples.iter_mut().find(|s| s.id == sid) {
            sample.name = r.name.clone();
        }
        let tree = trees[k].iter().map(|n| SampleGate::new(n.clone(), tailored[k].contains(&n.id))).collect();
        ws.sample_gates.insert(sid.clone(), tree);
        for (ch, t) in &r.tr.transforms {
            if ws.channel_settings.get(ch).map(|c| &c.transform) != Some(t) {
                ws.sample_channel_settings.entry(sid.clone()).or_default().insert(ch.clone(), ChannelSetting { transform: t.clone() });
            }
        }
    }
    for g in groups {
        if g.name == ALL_SAMPLES_NAME {
            ws.groups[0].gates = g.tree;
            continue;
        }
        let members: Vec<String> = g.members.iter().filter_map(|m| wsp_to_sid.get(m).cloned()).collect();
        let gid = ws.create_group(&g.name, &members)?;
        if let Some(grp) = ws.groups.iter_mut().find(|x| x.id == gid) {
            grp.gates = g.tree;
        }
    }
    if let Some(m) = matrices.into_iter().next() {
        ws.set_compensation(CompSource::Custom { matrix: m });
    } else {
        ws.set_compensation(CompSource::None);
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
