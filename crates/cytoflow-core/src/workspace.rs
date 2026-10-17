//! The analysis workspace: samples, per-channel transforms, compensation and
//! the shared gate hierarchy, plus cached evaluation of populations.

use crate::compensation::CompMatrix;
use crate::error::{Error, Result};
use crate::fcs::FcsFile;
use crate::gate::{BoolOp, GateDim, GateNode, GateShape};
use crate::stats::{channel_stats, ChannelStats};
use crate::transform::{Transform, TransformKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSetting {
    pub transform: Transform,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SampleInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub event_count: usize,
    #[serde(default)]
    pub channels: Vec<crate::fcs::Parameter>,
    /// A few informative keywords (`$CYT`, `$DATE`, `$FIL`, ...).
    #[serde(default)]
    pub keywords: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum CompSource {
    /// No compensation.
    #[default]
    None,
    /// Use each file's own `$SPILLOVER` keyword.
    Fcs,
    /// A single user-defined matrix applied to all samples.
    Custom { matrix: CompMatrix },
}

/// Serialisable workspace document.
#[derive(Debug, Serialize, Deserialize)]
pub struct Workspace {
    pub version: u32,
    pub name: String,
    pub samples: Vec<SampleInfo>,
    /// Transform per channel name (shared by its compensated variant).
    pub channel_settings: BTreeMap<String, ChannelSetting>,
    pub compensation: CompSource,
    pub gates: Vec<GateNode>,
    #[serde(skip)]
    loaded: HashMap<String, LoadedSample>,
    #[serde(skip)]
    next_id: u64,
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace {
            version: 1,
            name: "Untitled".into(),
            samples: Vec::new(),
            channel_settings: BTreeMap::new(),
            compensation: CompSource::None,
            gates: Vec::new(),
            loaded: HashMap::new(),
            next_id: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PopulationStats {
    pub gate_id: Option<String>,
    pub name: String,
    pub count: usize,
    pub parent_count: usize,
    pub total: usize,
    pub pct_parent: f64,
    pub pct_total: f64,
}

#[derive(Debug, Clone)]
pub struct LoadedSample {
    pub fcs: Arc<FcsFile>,
    compensated: Option<Arc<Vec<Vec<f32>>>>,
    transformed: HashMap<(String, bool), Arc<Vec<f32>>>,
    membership: HashMap<String, Arc<Vec<bool>>>,
}

impl LoadedSample {
    fn new(fcs: FcsFile) -> Self {
        LoadedSample { fcs: Arc::new(fcs), compensated: None, transformed: HashMap::new(), membership: HashMap::new() }
    }
    fn invalidate_all(&mut self) {
        self.compensated = None;
        self.transformed.clear();
        self.membership.clear();
    }
}

fn is_scatter_or_time(name: &str) -> bool {
    let u = name.to_uppercase();
    u.starts_with("FSC") || u.starts_with("SSC") || u == "TIME" || u.starts_with("TIME") || u.starts_with("EVENT")
}

impl Workspace {
    pub fn new(name: &str) -> Self {
        Workspace { name: name.to_string(), ..Default::default() }
    }

    // ---------------------------------------------------------------- samples

    fn fresh_id(&mut self, prefix: &str) -> String {
        loop {
            let id = format!("{prefix}{}", self.next_id);
            self.next_id += 1;
            if !self.samples.iter().any(|s| s.id == id) && !self.gates.iter().any(|g| g.id == id) {
                return id;
            }
        }
    }

    /// Load an FCS file and add it as a sample. Returns the sample id.
    pub fn add_sample<P: AsRef<Path>>(&mut self, path: P) -> Result<String> {
        let path = path.as_ref().to_path_buf();
        let fcs = FcsFile::open(&path)?;
        let id = self.fresh_id("s");
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| id.clone());
        let mut keywords = BTreeMap::new();
        for k in ["$CYT", "$DATE", "$BTIM", "$ETIM", "$FIL", "$SRC", "$EXP", "$OP", "$CYTSN", "$INST", "$PROJ", "TUBE NAME", "EXPERIMENT NAME"] {
            if let Some(v) = fcs.keyword(k) {
                keywords.insert(k.to_string(), v.trim().to_string());
            }
        }
        let info = SampleInfo { id: id.clone(), name, path, event_count: fcs.event_count, channels: fcs.parameters.clone(), keywords };
        self.ensure_channel_defaults(&fcs);
        if matches!(self.compensation, CompSource::None) && fcs.spillover.as_ref().is_some_and(|s| !s.matrix.is_empty()) {
            self.compensation = CompSource::Fcs;
        }
        self.samples.push(info);
        self.loaded.insert(id.clone(), LoadedSample::new(fcs));
        Ok(id)
    }

    /// Re-open the FCS files listed in a deserialised workspace. Missing files
    /// are returned as errors but do not abort the rest.
    pub fn reload_samples(&mut self, base_dir: Option<&Path>) -> Vec<(String, Error)> {
        let mut errors = Vec::new();
        let infos: Vec<(String, PathBuf)> = self.samples.iter().map(|s| (s.id.clone(), s.path.clone())).collect();
        for (id, path) in infos {
            let candidate = match base_dir {
                Some(b) if !path.is_absolute() => b.join(&path),
                _ => path.clone(),
            };
            let path = if candidate.exists() { candidate } else if let Some(b) = base_dir {
                let alt = b.join(path.file_name().unwrap_or_default());
                if alt.exists() { alt } else { candidate }
            } else { candidate };
            match FcsFile::open(&path) {
                Ok(fcs) => {
                    self.ensure_channel_defaults(&fcs);
                    if let Some(s) = self.samples.iter_mut().find(|s| s.id == id) {
                        s.event_count = fcs.event_count;
                        s.channels = fcs.parameters.clone();
                        s.path = path;
                    }
                    self.loaded.insert(id, LoadedSample::new(fcs));
                }
                Err(e) => errors.push((id, e)),
            }
        }
        let max_id = self.samples.iter().chain_ids().chain(self.gates.iter().map(|g| g.id.as_str()))
            .filter_map(|id| id.trim_start_matches(|c: char| c.is_alphabetic()).parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(max_id + 1);
        errors
    }

    pub fn remove_sample(&mut self, id: &str) -> Result<()> {
        let idx = self.samples.iter().position(|s| s.id == id).ok_or_else(|| Error::SampleNotFound(id.into()))?;
        self.samples.remove(idx);
        self.loaded.remove(id);
        Ok(())
    }

    pub fn is_loaded(&self, id: &str) -> bool {
        self.loaded.contains_key(id)
    }

    pub fn fcs(&self, id: &str) -> Result<Arc<FcsFile>> {
        self.loaded.get(id).map(|l| l.fcs.clone()).ok_or_else(|| Error::SampleNotFound(id.into()))
    }

    fn ensure_channel_defaults(&mut self, fcs: &FcsFile) {
        for (i, p) in fcs.parameters.iter().enumerate() {
            if self.channel_settings.contains_key(&p.name) {
                continue;
            }
            let kind = if is_scatter_or_time(&p.name) { TransformKind::Linear } else { TransformKind::Logicle };
            let range = if p.range > 1.0 { p.range } else { 262144.0 };
            let t = Transform::default_for(kind, range, Some(&fcs.data[i]));
            self.channel_settings.insert(p.name.clone(), ChannelSetting { transform: t });
        }
    }

    // ------------------------------------------------------------- transforms

    pub fn transform_for(&self, channel: &str) -> Transform {
        self.channel_settings.get(channel).map(|c| c.transform.clone()).unwrap_or_default()
    }

    /// Change a channel's transform, re-mapping gate coordinates so that gates
    /// keep the same raw-data meaning.
    pub fn set_transform(&mut self, channel: &str, transform: Transform) {
        let old = self.transform_for(channel);
        if old == transform {
            return;
        }
        for g in self.gates.iter_mut() {
            remap_gate(&mut g.gate.shape, channel, &old, &transform);
        }
        self.channel_settings.insert(channel.to_string(), ChannelSetting { transform });
        for l in self.loaded.values_mut() {
            l.transformed.retain(|(c, _), _| c != channel);
            l.membership.clear();
        }
    }

    pub fn set_compensation(&mut self, comp: CompSource) {
        self.compensation = comp;
        for l in self.loaded.values_mut() {
            l.invalidate_all();
        }
    }

    /// The effective compensation matrix for a sample, if any.
    pub fn comp_matrix_for(&self, sample_id: &str) -> Option<CompMatrix> {
        match &self.compensation {
            CompSource::None => None,
            CompSource::Custom { matrix } => Some(matrix.clone()),
            CompSource::Fcs => self.loaded.get(sample_id).and_then(|l| l.fcs.spillover.clone()).map(CompMatrix::from),
        }
    }

    // ------------------------------------------------------------------ data

    /// Raw or compensated column.
    pub fn column(&mut self, sample_id: &str, channel: &str, compensated: bool) -> Result<Arc<Vec<f32>>> {
        let comp = if compensated { self.comp_matrix_for(sample_id) } else { None };
        let l = self.loaded.get_mut(sample_id).ok_or_else(|| Error::SampleNotFound(sample_id.into()))?;
        let idx = l.fcs.channel_index(channel).ok_or_else(|| Error::ChannelNotFound(channel.into()))?;
        match comp {
            Some(m) if m.channels.iter().any(|c| c == channel) => {
                if l.compensated.is_none() {
                    let cols = m.apply(&l.fcs)?;
                    l.compensated = Some(Arc::new(cols));
                }
                Ok(Arc::new(l.compensated.as_ref().unwrap()[idx].clone()))
            }
            _ => Ok(Arc::new(l.fcs.data[idx].clone())),
        }
    }

    /// Transformed (display-space) column, cached.
    pub fn transformed(&mut self, sample_id: &str, channel: &str, compensated: bool) -> Result<Arc<Vec<f32>>> {
        let key = (channel.to_string(), compensated);
        if let Some(l) = self.loaded.get(sample_id) {
            if let Some(v) = l.transformed.get(&key) {
                return Ok(v.clone());
            }
        }
        let raw = self.column(sample_id, channel, compensated)?;
        let t = self.transform_for(channel);
        let mut out = Vec::new();
        t.apply_slice(&raw, &mut out);
        let arc = Arc::new(out);
        self.loaded.get_mut(sample_id).unwrap().transformed.insert(key, arc.clone());
        Ok(arc)
    }

    // ----------------------------------------------------------------- gates

    pub fn gate(&self, id: &str) -> Result<&GateNode> {
        self.gates.iter().find(|g| g.id == id).ok_or_else(|| Error::GateNotFound(id.into()))
    }

    pub fn children_of(&self, parent: Option<&str>) -> Vec<&GateNode> {
        self.gates.iter().filter(|g| g.parent.as_deref() == parent).collect()
    }

    /// Add a gate; returns its id.
    pub fn add_gate(&mut self, name: &str, parent: Option<&str>, shape: GateShape, negate: bool) -> Result<String> {
        if let Some(p) = parent {
            self.gate(p)?;
        }
        validate_shape(&shape)?;
        let id = self.fresh_id("g");
        self.gates.push(GateNode { id: id.clone(), name: name.to_string(), parent: parent.map(String::from), gate: crate::gate::Gate { shape, negate, grid: None }, color: None });
        self.invalidate_memberships();
        Ok(id)
    }

    /// `color`: `None` leaves the colour unchanged, `Some(None)` clears it.
    pub fn update_gate(&mut self, id: &str, shape: Option<GateShape>, name: Option<&str>, negate: Option<bool>, color: Option<Option<String>>) -> Result<()> {
        let g = self.gates.iter_mut().find(|g| g.id == id).ok_or_else(|| Error::GateNotFound(id.into()))?;
        if let Some(c) = color {
            g.color = c;
        }
        if let Some(s) = shape {
            validate_shape(&s)?;
            g.gate.shape = s;
        }
        if let Some(n) = name {
            g.name = n.to_string();
        }
        if let Some(n) = negate {
            g.gate.negate = n;
        }
        self.invalidate_memberships();
        Ok(())
    }

    /// Remove a gate and all of its descendants. Returns removed ids.
    pub fn remove_gate(&mut self, id: &str) -> Result<Vec<String>> {
        self.gate(id)?;
        let mut doomed: HashSet<String> = HashSet::from([id.to_string()]);
        loop {
            let before = doomed.len();
            for g in &self.gates {
                if let Some(p) = &g.parent {
                    if doomed.contains(p) {
                        doomed.insert(g.id.clone());
                    }
                }
            }
            if doomed.len() == before {
                break;
            }
        }
        self.gates.retain(|g| !doomed.contains(&g.id));
        self.invalidate_memberships();
        Ok(doomed.into_iter().collect())
    }

    fn invalidate_memberships(&mut self) {
        for l in self.loaded.values_mut() {
            l.membership.clear();
        }
    }

    /// Path of names from the root to this gate, joined by `/`.
    pub fn gate_path(&self, id: &str) -> String {
        let mut parts = Vec::new();
        let mut cur = Some(id.to_string());
        while let Some(c) = cur {
            match self.gates.iter().find(|g| g.id == c) {
                Some(g) => {
                    parts.push(g.name.clone());
                    cur = g.parent.clone();
                }
                None => break,
            }
        }
        parts.reverse();
        parts.join("/")
    }

    /// Boolean membership vector for a population (gate id), including parents.
    pub fn membership(&mut self, sample_id: &str, gate_id: &str) -> Result<Arc<Vec<bool>>> {
        if let Some(l) = self.loaded.get(sample_id) {
            if let Some(m) = l.membership.get(gate_id) {
                return Ok(m.clone());
            }
        }
        let node = self.gate(gate_id)?.clone();
        let n = self.fcs(sample_id)?.event_count;
        let mut own = vec![false; n];
        match &node.gate.shape {
            GateShape::Boolean { op, refs } => {
                let members: Vec<Arc<Vec<bool>>> = refs.iter().map(|r| self.membership(sample_id, r)).collect::<Result<_>>()?;
                match op {
                    BoolOp::And => {
                        for v in own.iter_mut() {
                            *v = true;
                        }
                        for m in &members {
                            for i in 0..n {
                                own[i] &= m[i];
                            }
                        }
                        if members.is_empty() {
                            own.iter_mut().for_each(|v| *v = false);
                        }
                    }
                    BoolOp::Or => {
                        for m in &members {
                            for i in 0..n {
                                own[i] |= m[i];
                            }
                        }
                    }
                    BoolOp::Not => {
                        if let Some(m) = members.first() {
                            for i in 0..n {
                                own[i] = !m[i];
                            }
                        }
                    }
                }
            }
            shape => {
                let dims: Vec<GateDim> = shape.dims().into_iter().cloned().collect();
                let mut cols: Vec<Arc<Vec<f32>>> = dims.iter().map(|d| self.transformed(sample_id, &d.channel, d.compensated)).collect::<Result<_>>()?;
                if let (Some(g), GateShape::Polygon { .. }) = (node.gate.grid, shape) {
                    cols = cols.iter().map(|c| Arc::new(crate::gate::snap_to_grid(c, g))).collect();
                }
                let refs: Vec<&[f32]> = cols.iter().map(|c| c.as_slice()).collect();
                shape.evaluate(&refs, &mut own);
            }
        }
        if node.gate.negate {
            own.iter_mut().for_each(|v| *v = !*v);
        }
        if let Some(p) = &node.parent {
            let pm = self.membership(sample_id, p)?;
            for i in 0..n {
                own[i] &= pm[i];
            }
        }
        let arc = Arc::new(own);
        self.loaded.get_mut(sample_id).unwrap().membership.insert(gate_id.to_string(), arc.clone());
        Ok(arc)
    }

    pub fn count(&mut self, sample_id: &str, gate_id: Option<&str>) -> Result<usize> {
        match gate_id {
            None => Ok(self.fcs(sample_id)?.event_count),
            Some(g) => Ok(self.membership(sample_id, g)?.iter().filter(|&&b| b).count()),
        }
    }

    pub fn population_stats(&mut self, sample_id: &str, gate_id: Option<&str>) -> Result<PopulationStats> {
        let total = self.fcs(sample_id)?.event_count;
        let (name, parent) = match gate_id {
            None => ("All events".to_string(), None),
            Some(g) => {
                let node = self.gate(g)?;
                (node.name.clone(), node.parent.clone())
            }
        };
        let count = self.count(sample_id, gate_id)?;
        let parent_count = if gate_id.is_none() { total } else { self.count(sample_id, parent.as_deref())? };
        Ok(PopulationStats {
            gate_id: gate_id.map(String::from),
            name,
            count,
            parent_count,
            total,
            pct_parent: if parent_count > 0 { 100.0 * count as f64 / parent_count as f64 } else { 0.0 },
            pct_total: if total > 0 { 100.0 * count as f64 / total as f64 } else { 0.0 },
        })
    }

    /// Stats for every population in the tree (pre-order), for one sample.
    pub fn all_population_stats(&mut self, sample_id: &str) -> Result<Vec<PopulationStats>> {
        let mut out = vec![self.population_stats(sample_id, None)?];
        let order = self.preorder();
        for id in order {
            out.push(self.population_stats(sample_id, Some(&id))?);
        }
        Ok(out)
    }

    /// Gate ids in depth-first pre-order.
    pub fn preorder(&self) -> Vec<String> {
        fn walk(ws: &Workspace, parent: Option<&str>, out: &mut Vec<String>) {
            for g in ws.children_of(parent) {
                out.push(g.id.clone());
                walk(ws, Some(&g.id), out);
            }
        }
        let mut out = Vec::new();
        walk(self, None, &mut out);
        out
    }

    pub fn channel_statistics(&mut self, sample_id: &str, gate_id: Option<&str>, channel: &str, compensated: bool) -> Result<ChannelStats> {
        let col = self.column(sample_id, channel, compensated)?;
        let mask = match gate_id {
            Some(g) => Some(self.membership(sample_id, g)?),
            None => None,
        };
        Ok(channel_stats(channel, &col, mask.as_deref().map(|v| v.as_slice())))
    }

    // -------------------------------------------------------- serialisation

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let mut ws: Workspace = serde_json::from_str(json)?;
        ws.next_id = 1;
        Ok(ws)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<(Self, Vec<(String, Error)>)> {
        let p = path.as_ref();
        let text = std::fs::read_to_string(p)?;
        let mut ws = Self::from_json(&text)?;
        let errors = ws.reload_samples(p.parent());
        Ok((ws, errors))
    }
}

trait ChainIds<'a> {
    fn chain_ids(self) -> Box<dyn Iterator<Item = &'a str> + 'a>;
}
impl<'a, I: Iterator<Item = &'a SampleInfo> + 'a> ChainIds<'a> for I {
    fn chain_ids(self) -> Box<dyn Iterator<Item = &'a str> + 'a> {
        Box::new(self.map(|s| s.id.as_str()))
    }
}

fn validate_shape(shape: &GateShape) -> Result<()> {
    match shape {
        GateShape::Rectangle { dims, min, max } => {
            if dims.is_empty() || dims.len() > 2 || min.len() != dims.len() || max.len() != dims.len() {
                return Err(Error::InvalidGate("rectangle needs 1 or 2 dimensions with matching bounds".into()));
            }
        }
        GateShape::Polygon { vertices, .. } => {
            if vertices.len() < 3 {
                return Err(Error::InvalidGate("polygon needs at least 3 vertices".into()));
            }
        }
        GateShape::Ellipse { a, b, .. } => {
            if *a <= 0.0 || *b <= 0.0 {
                return Err(Error::InvalidGate("ellipse axes must be positive".into()));
            }
        }
        GateShape::Boolean { refs, op } => {
            if refs.is_empty() || (*op == BoolOp::Not && refs.len() != 1) {
                return Err(Error::InvalidGate("boolean gate needs references".into()));
            }
        }
    }
    Ok(())
}

/// Re-express a gate's coordinates on `channel` from transform `old` to `new`.
fn remap_gate(shape: &mut GateShape, channel: &str, old: &Transform, new: &Transform) {
    let map = |v: f64| new.apply(old.invert(v));
    match shape {
        GateShape::Rectangle { dims, min, max } => {
            for (k, d) in dims.iter().enumerate() {
                if d.channel == channel {
                    if let Some(v) = min[k] {
                        min[k] = Some(map(v));
                    }
                    if let Some(v) = max[k] {
                        max[k] = Some(map(v));
                    }
                }
            }
        }
        GateShape::Polygon { dims, vertices } => {
            for k in 0..2 {
                if dims[k].channel == channel {
                    for v in vertices.iter_mut() {
                        v[k] = map(v[k]);
                    }
                }
            }
        }
        GateShape::Ellipse { dims, center, a, b, angle } => {
            // Approximate: convert to a polygon-like sampling of the boundary, remap, refit.
            let affects = dims.iter().any(|d| d.channel == channel);
            if !affects {
                return;
            }
            let (sin, cos) = angle.sin_cos();
            let mut pts: Vec<[f64; 2]> = (0..64)
                .map(|i| {
                    let th = i as f64 / 64.0 * std::f64::consts::TAU;
                    let (u, v) = (*a * th.cos(), *b * th.sin());
                    [center[0] + u * cos - v * sin, center[1] + u * sin + v * cos]
                })
                .collect();
            for p in pts.iter_mut() {
                for k in 0..2 {
                    if dims[k].channel == channel {
                        p[k] = map(p[k]);
                    }
                }
            }
            // Refit: centre = mean, axes/angle from covariance.
            let n = pts.len() as f64;
            let cx = pts.iter().map(|p| p[0]).sum::<f64>() / n;
            let cy = pts.iter().map(|p| p[1]).sum::<f64>() / n;
            let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
            for p in &pts {
                sxx += (p[0] - cx).powi(2);
                sxy += (p[0] - cx) * (p[1] - cy);
                syy += (p[1] - cy).powi(2);
            }
            // For points uniformly on an ellipse, cov = diag(a²/2, b²/2) rotated.
            let cov = [[sxx / n * 2.0, sxy / n * 2.0], [sxy / n * 2.0, syy / n * 2.0]];
            if let GateShape::Ellipse { center: c2, a: a2, b: b2, angle: an2, .. } =
                GateShape::ellipse_from_covariance(dims.clone(), [cx, cy], cov, 1.0)
            {
                *center = c2;
                *a = a2;
                *b = b2;
                *angle = an2;
            }
        }
        GateShape::Boolean { .. } => {}
    }
}
