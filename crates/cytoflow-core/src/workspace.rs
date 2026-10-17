//! The analysis workspace: samples, per-channel transforms, compensation,
//! sample groups with template gate trees, the gate tree of every sample, and
//! cached evaluation of populations.
//!
//! ## Groups and per-sample trees
//!
//! Every sample owns its effective gate tree (`sample_gates`). Groups
//! (`groups`) hold *template* trees: adding a gate to a group copies it, with
//! the same id, into every member sample; editing it through the group updates
//! every member's copy that has not been tailored; removing it removes the
//! copies (and their descendants). Editing a gate on a single sample only
//! changes that sample's copy and marks it `tailored`. The built-in group
//! [`ALL_SAMPLES`] always exists and contains every sample.
//!
//! Gate coordinates are in display space. Group templates use the workspace
//! default transforms (`channel_settings`); a sample's tree uses that
//! sample's effective transforms (its overrides in `sample_channel_settings`,
//! falling back to the defaults). Copying a gate between a group and a sample
//! re-maps the coordinates of overridden channels.

use crate::compensation::CompMatrix;
use crate::error::{Error, Result};
use crate::fcs::FcsFile;
use crate::gate::{BoolOp, GateDim, GateNode, GateShape};
use crate::stats::{channel_stats, ChannelStats};
use crate::transform::{Transform, TransformKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Current workspace file format version.
pub const WORKSPACE_VERSION: u32 = 2;
/// Id of the built-in group that contains every sample.
pub const ALL_SAMPLES: &str = "all";
/// Display name of the built-in group.
pub const ALL_SAMPLES_NAME: &str = "All Samples";

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

/// A sample group with its template gate tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub sample_ids: Vec<String>,
    /// Template gates, in the workspace default transform space.
    #[serde(default)]
    pub gates: Vec<GateNode>,
}

/// A node of a sample's own gate tree: a [`GateNode`] plus whether it was
/// tailored, i.e. deviates from (or is absent from) the templates of the
/// sample's groups. Serialises as a `GateNode` with an extra `tailored` field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SampleGate {
    #[serde(flatten)]
    pub node: GateNode,
    #[serde(default)]
    pub tailored: bool,
}

impl SampleGate {
    pub fn new(node: GateNode, tailored: bool) -> Self {
        SampleGate { node, tailored }
    }
}

impl Deref for SampleGate {
    type Target = GateNode;
    fn deref(&self) -> &GateNode {
        &self.node
    }
}

impl DerefMut for SampleGate {
    fn deref_mut(&mut self) -> &mut GateNode {
        &mut self.node
    }
}

impl AsRef<GateNode> for SampleGate {
    fn as_ref(&self) -> &GateNode {
        &self.node
    }
}

impl AsRef<GateNode> for GateNode {
    fn as_ref(&self) -> &GateNode {
        self
    }
}

/// Where a gate edit applies: a group template (and its members' copies), or a
/// single sample's tree. JSON: `{ "group": id }` or `{ "sample": id }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateScope {
    Group(String),
    Sample(String),
}

impl Default for GateScope {
    fn default() -> Self {
        GateScope::Group(ALL_SAMPLES.into())
    }
}

/// A named layout-editor page. The core does not interpret `data`: it is the
/// frontend's layout document (items, page size, ...) stored verbatim.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Layout {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

/// Serialisable workspace document.
#[derive(Debug, Serialize, Deserialize)]
pub struct Workspace {
    pub version: u32,
    pub name: String,
    pub samples: Vec<SampleInfo>,
    /// Default transform per channel name (shared by its compensated variant).
    pub channel_settings: BTreeMap<String, ChannelSetting>,
    /// Per-sample transform overrides: sample id → channel → setting.
    #[serde(default)]
    pub sample_channel_settings: BTreeMap<String, BTreeMap<String, ChannelSetting>>,
    pub compensation: CompSource,
    /// Sample groups; the first is always [`ALL_SAMPLES`].
    #[serde(default)]
    pub groups: Vec<Group>,
    /// The effective gate tree of each sample (flat list, tree via `parent`).
    #[serde(default)]
    pub sample_gates: BTreeMap<String, Vec<SampleGate>>,
    /// Named layouts of the layout editor (opaque to the core).
    #[serde(default)]
    pub layouts: Vec<Layout>,
    /// Small UI preferences (last axes per sample, overlay selections, ...);
    /// opaque to the core. `null` when never set.
    #[serde(default)]
    pub ui_state: serde_json::Value,
    /// Version-1 shared gate list; migrated into the "All Samples" group on load.
    #[serde(default, rename = "gates", skip_serializing)]
    legacy_gates: Vec<GateNode>,
    #[serde(skip)]
    loaded: HashMap<String, LoadedSample>,
    #[serde(skip)]
    next_id: u64,
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace {
            version: WORKSPACE_VERSION,
            name: "Untitled".into(),
            samples: Vec::new(),
            channel_settings: BTreeMap::new(),
            sample_channel_settings: BTreeMap::new(),
            compensation: CompSource::None,
            groups: vec![Group { id: ALL_SAMPLES.into(), name: ALL_SAMPLES_NAME.into(), sample_ids: Vec::new(), gates: Vec::new() }],
            sample_gates: BTreeMap::new(),
            layouts: Vec::new(),
            ui_state: serde_json::Value::Null,
            legacy_gates: Vec::new(),
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

/// Ids of `id` and all of its descendants in a flat tree.
fn subtree<T: AsRef<GateNode>>(nodes: &[T], id: &str) -> HashSet<String> {
    let mut doomed: HashSet<String> = HashSet::from([id.to_string()]);
    loop {
        let before = doomed.len();
        for g in nodes {
            let g = g.as_ref();
            if g.parent.as_ref().is_some_and(|p| doomed.contains(p)) {
                doomed.insert(g.id.clone());
            }
        }
        if doomed.len() == before {
            return doomed;
        }
    }
}

/// Gate ids of a flat tree in depth-first pre-order.
fn preorder_of<T: AsRef<GateNode>>(nodes: &[T]) -> Vec<String> {
    fn walk<T: AsRef<GateNode>>(nodes: &[T], parent: Option<&str>, out: &mut Vec<String>) {
        for g in nodes.iter().map(AsRef::as_ref).filter(|g| g.parent.as_deref() == parent) {
            out.push(g.id.clone());
            walk(nodes, Some(&g.id), out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, None, &mut out);
    out
}

fn path_of<T: AsRef<GateNode>>(nodes: &[T], id: &str) -> String {
    let mut parts = Vec::new();
    let mut cur = Some(id.to_string());
    while let Some(c) = cur {
        match nodes.iter().map(AsRef::as_ref).find(|g| g.id == c) {
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

impl Workspace {
    pub fn new(name: &str) -> Self {
        Workspace { name: name.to_string(), ..Default::default() }
    }

    // ---------------------------------------------------------------- ids

    fn id_in_use(&self, id: &str) -> bool {
        self.samples.iter().any(|s| s.id == id)
            || self.groups.iter().any(|g| g.id == id || g.gates.iter().any(|n| n.id == id))
            || self.sample_gates.values().any(|t| t.iter().any(|n| n.id == id))
    }

    fn fresh_id(&mut self, prefix: &str) -> String {
        loop {
            let id = format!("{prefix}{}", self.next_id);
            self.next_id += 1;
            if !self.id_in_use(&id) {
                return id;
            }
        }
    }

    /// Bring a deserialised (possibly version-1) workspace to the current
    /// model: the "All Samples" group exists first and contains every sample,
    /// groups only list existing samples, every sample has a gate tree.
    fn normalize(&mut self) {
        let legacy = std::mem::take(&mut self.legacy_gates);
        match self.groups.iter().position(|g| g.id == ALL_SAMPLES) {
            Some(0) => {}
            Some(i) => {
                let g = self.groups.remove(i);
                self.groups.insert(0, g);
            }
            None => self.groups.insert(0, Group { id: ALL_SAMPLES.into(), name: ALL_SAMPLES_NAME.into(), sample_ids: Vec::new(), gates: legacy }),
        }
        let ids: Vec<String> = self.samples.iter().map(|s| s.id.clone()).collect();
        self.groups[0].sample_ids = ids.clone();
        for g in self.groups.iter_mut().skip(1) {
            g.sample_ids.retain(|s| ids.contains(s));
        }
        self.sample_gates.retain(|k, _| ids.contains(k));
        self.sample_channel_settings.retain(|k, _| ids.contains(k));
        for sid in &ids {
            if !self.sample_gates.contains_key(sid) {
                let tree = self.groups[0].gates.iter().map(|n| SampleGate::new(self.template_to_sample(sid, n), false)).collect();
                self.sample_gates.insert(sid.clone(), tree);
            }
        }
        self.version = WORKSPACE_VERSION;
    }

    // ---------------------------------------------------------------- samples

    fn register_sample(&mut self, info: SampleInfo) -> String {
        let id = info.id.clone();
        self.samples.push(info);
        if self.groups.first().is_none_or(|g| g.id != ALL_SAMPLES) {
            self.normalize();
        }
        self.groups[0].sample_ids.push(id.clone());
        let tree = self.groups[0].gates.iter().map(|n| SampleGate::new(self.template_to_sample(&id, n), false)).collect();
        self.sample_gates.insert(id.clone(), tree);
        id
    }

    /// Load an FCS file and add it as a sample (member of "All Samples", with
    /// a copy of that group's gates). Returns the sample id.
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
        self.loaded.insert(id.clone(), LoadedSample::new(fcs));
        Ok(self.register_sample(info))
    }

    /// Add a sample whose FCS file is not available (it is listed, keeps its
    /// gate tree and can be re-attached by [`Workspace::reload_samples`]).
    pub fn add_unloaded_sample(&mut self, name: &str, path: &Path) -> String {
        let id = self.fresh_id("s");
        self.register_sample(SampleInfo { id, name: name.to_string(), path: path.to_path_buf(), event_count: 0, channels: Vec::new(), keywords: BTreeMap::new() })
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
        errors
    }

    pub fn remove_sample(&mut self, id: &str) -> Result<()> {
        let idx = self.samples.iter().position(|s| s.id == id).ok_or_else(|| Error::SampleNotFound(id.into()))?;
        self.samples.remove(idx);
        self.loaded.remove(id);
        self.sample_gates.remove(id);
        self.sample_channel_settings.remove(id);
        for g in self.groups.iter_mut() {
            g.sample_ids.retain(|s| s != id);
        }
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

    // ---------------------------------------------------------------- groups

    pub fn group(&self, group_id: &str) -> Result<&Group> {
        self.groups.iter().find(|g| g.id == group_id).ok_or_else(|| Error::Workspace(format!("group not found: {group_id}")))
    }

    fn group_index(&self, group_id: &str) -> Result<usize> {
        self.groups.iter().position(|g| g.id == group_id).ok_or_else(|| Error::Workspace(format!("group not found: {group_id}")))
    }

    fn check_samples(&self, sample_ids: &[String]) -> Result<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        for s in sample_ids {
            if !self.samples.iter().any(|x| &x.id == s) {
                return Err(Error::SampleNotFound(s.clone()));
            }
            if !out.contains(s) {
                out.push(s.clone());
            }
        }
        Ok(out)
    }

    /// Create a group (with an empty template). Returns its id.
    pub fn create_group(&mut self, name: &str, sample_ids: &[String]) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Workspace("group name must not be empty".into()));
        }
        let members = self.check_samples(sample_ids)?;
        let id = self.fresh_id("grp");
        self.groups.push(Group { id: id.clone(), name: name.to_string(), sample_ids: members, gates: Vec::new() });
        Ok(id)
    }

    /// Replace the members of a group. New members receive copies of the
    /// template gates they do not have yet; removed members keep their gates.
    pub fn set_group_members(&mut self, group_id: &str, sample_ids: &[String]) -> Result<()> {
        if group_id == ALL_SAMPLES {
            return Err(Error::Workspace("the All Samples group always contains every sample".into()));
        }
        let gi = self.group_index(group_id)?;
        let members = self.check_samples(sample_ids)?;
        let added: Vec<String> = members.iter().filter(|s| !self.groups[gi].sample_ids.contains(s)).cloned().collect();
        self.groups[gi].sample_ids = members;
        let template = self.groups[gi].gates.clone();
        for sid in &added {
            for id in preorder_of(&template) {
                let node = template.iter().find(|n| n.id == id).unwrap();
                self.insert_template_copy(sid, node);
            }
            self.invalidate_sample(sid);
        }
        Ok(())
    }

    /// Delete a group (its members keep their gates).
    pub fn remove_group(&mut self, group_id: &str) -> Result<()> {
        if group_id == ALL_SAMPLES {
            return Err(Error::Workspace("the All Samples group cannot be removed".into()));
        }
        let gi = self.group_index(group_id)?;
        self.groups.remove(gi);
        Ok(())
    }

    // ------------------------------------------------------------- transforms

    /// The workspace default transform of a channel.
    pub fn default_transform(&self, channel: &str) -> Transform {
        self.channel_settings.get(channel).map(|c| c.transform.clone()).unwrap_or_default()
    }

    /// The effective transform of a channel for a sample: its override if any,
    /// else the workspace default.
    pub fn transform_for(&self, sample_id: &str, channel: &str) -> Transform {
        self.sample_channel_settings
            .get(sample_id)
            .and_then(|m| m.get(channel))
            .map(|c| c.transform.clone())
            .unwrap_or_else(|| self.default_transform(channel))
    }

    fn has_override(&self, sample_id: &str, channel: &str) -> bool {
        self.sample_channel_settings.get(sample_id).is_some_and(|m| m.contains_key(channel))
    }

    /// Change a channel's default transform, re-mapping the coordinates of the
    /// group templates and of every sample tree that uses the default on that
    /// channel (samples with an override keep theirs), so that gates keep the
    /// same raw-data meaning.
    pub fn set_transform(&mut self, channel: &str, transform: Transform) {
        let old = self.default_transform(channel);
        if old == transform {
            return;
        }
        for g in self.groups.iter_mut() {
            for n in g.gates.iter_mut() {
                remap_gate(&mut n.gate.shape, channel, &old, &transform);
            }
        }
        let followers: Vec<String> = self.samples.iter().filter(|s| !self.has_override(&s.id, channel)).map(|s| s.id.clone()).collect();
        for sid in &followers {
            if let Some(tree) = self.sample_gates.get_mut(sid) {
                for n in tree.iter_mut() {
                    remap_gate(&mut n.node.gate.shape, channel, &old, &transform);
                }
            }
            if let Some(l) = self.loaded.get_mut(sid) {
                l.transformed.retain(|(c, _), _| c != channel);
                l.membership.clear();
            }
        }
        self.channel_settings.insert(channel.to_string(), ChannelSetting { transform });
    }

    /// Override a channel's transform for one sample, re-mapping only that
    /// sample's gates. Setting the default transform removes the override.
    pub fn set_sample_transform(&mut self, sample_id: &str, channel: &str, transform: Transform) -> Result<()> {
        if !self.samples.iter().any(|s| s.id == sample_id) {
            return Err(Error::SampleNotFound(sample_id.into()));
        }
        let old = self.transform_for(sample_id, channel);
        if transform == self.default_transform(channel) {
            if let Some(m) = self.sample_channel_settings.get_mut(sample_id) {
                m.remove(channel);
                if m.is_empty() {
                    self.sample_channel_settings.remove(sample_id);
                }
            }
        } else {
            self.sample_channel_settings
                .entry(sample_id.to_string())
                .or_default()
                .insert(channel.to_string(), ChannelSetting { transform: transform.clone() });
        }
        if old == transform {
            return Ok(());
        }
        if let Some(tree) = self.sample_gates.get_mut(sample_id) {
            for n in tree.iter_mut() {
                remap_gate(&mut n.node.gate.shape, channel, &old, &transform);
            }
        }
        if let Some(l) = self.loaded.get_mut(sample_id) {
            l.transformed.retain(|(c, _), _| c != channel);
            l.membership.clear();
        }
        Ok(())
    }

    /// Re-express a shape from the default transform space in the sample's space.
    fn shape_to_sample(&self, sample_id: &str, shape: &mut GateShape) {
        let channels: Vec<String> = shape.dims().iter().map(|d| d.channel.clone()).collect();
        for ch in dedup(channels) {
            if self.has_override(sample_id, &ch) {
                remap_gate(shape, &ch, &self.default_transform(&ch), &self.transform_for(sample_id, &ch));
            }
        }
    }

    /// Re-express a shape from the sample's transform space in the default space.
    fn shape_to_default(&self, sample_id: &str, shape: &mut GateShape) {
        let channels: Vec<String> = shape.dims().iter().map(|d| d.channel.clone()).collect();
        for ch in dedup(channels) {
            if self.has_override(sample_id, &ch) {
                remap_gate(shape, &ch, &self.transform_for(sample_id, &ch), &self.default_transform(&ch));
            }
        }
    }

    fn template_to_sample(&self, sample_id: &str, node: &GateNode) -> GateNode {
        let mut n = node.clone();
        self.shape_to_sample(sample_id, &mut n.gate.shape);
        n
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

    /// Transformed (display-space) column with the sample's effective transform, cached.
    pub fn transformed(&mut self, sample_id: &str, channel: &str, compensated: bool) -> Result<Arc<Vec<f32>>> {
        let key = (channel.to_string(), compensated);
        if let Some(l) = self.loaded.get(sample_id) {
            if let Some(v) = l.transformed.get(&key) {
                return Ok(v.clone());
            }
        }
        let raw = self.column(sample_id, channel, compensated)?;
        let t = self.transform_for(sample_id, channel);
        let mut out = Vec::new();
        t.apply_slice(&raw, &mut out);
        let arc = Arc::new(out);
        self.loaded.get_mut(sample_id).unwrap().transformed.insert(key, arc.clone());
        Ok(arc)
    }

    // ----------------------------------------------------------------- gates

    /// The gate tree of a sample.
    pub fn sample_tree(&self, sample_id: &str) -> Result<&[SampleGate]> {
        self.sample_gates.get(sample_id).map(|v| v.as_slice()).ok_or_else(|| Error::SampleNotFound(sample_id.into()))
    }

    /// The template tree of the "All Samples" group (the version-1 `gates`).
    pub fn gates(&self) -> &[GateNode] {
        self.groups.first().map(|g| g.gates.as_slice()).unwrap_or(&[])
    }

    /// A gate of a sample's tree.
    pub fn gate(&self, sample_id: &str, id: &str) -> Result<&GateNode> {
        self.sample_tree(sample_id)?.iter().find(|g| g.id == id).map(|g| &g.node).ok_or_else(|| Error::GateNotFound(id.into()))
    }

    /// A node of a sample's tree, with its `tailored` flag.
    pub fn sample_gate(&self, sample_id: &str, id: &str) -> Result<&SampleGate> {
        self.sample_tree(sample_id)?.iter().find(|g| g.id == id).ok_or_else(|| Error::GateNotFound(id.into()))
    }

    /// A template gate of a group.
    pub fn group_gate(&self, group_id: &str, id: &str) -> Result<&GateNode> {
        self.group(group_id)?.gates.iter().find(|g| g.id == id).ok_or_else(|| Error::GateNotFound(id.into()))
    }

    pub fn children_of(&self, sample_id: &str, parent: Option<&str>) -> Vec<&GateNode> {
        self.sample_gates.get(sample_id).map(|t| t.iter().map(|g| &g.node).filter(|g| g.parent.as_deref() == parent).collect()).unwrap_or_default()
    }

    /// Add a gate to the "All Samples" group (and so to every sample); returns its id.
    pub fn add_gate(&mut self, name: &str, parent: Option<&str>, shape: GateShape, negate: bool) -> Result<String> {
        self.add_gate_in(&GateScope::default(), name, parent, shape, negate)
    }

    /// Add a gate to a group template (copied into every member sample that
    /// has the parent population) or to a single sample (tailored). The shape
    /// is in the scope's transform space. Returns the new gate id.
    pub fn add_gate_in(&mut self, scope: &GateScope, name: &str, parent: Option<&str>, shape: GateShape, negate: bool) -> Result<String> {
        validate_shape(&shape)?;
        match scope {
            GateScope::Group(gid) => {
                let gi = self.group_index(gid)?;
                if let Some(p) = parent {
                    self.group_gate(gid, p)?;
                }
                let id = self.fresh_id("g");
                let node = GateNode { id: id.clone(), name: name.to_string(), parent: parent.map(String::from), gate: crate::gate::Gate { shape, negate, grid: None }, color: None };
                self.groups[gi].gates.push(node.clone());
                for sid in self.groups[gi].sample_ids.clone() {
                    self.insert_template_copy(&sid, &node);
                    self.invalidate_sample(&sid);
                }
                Ok(id)
            }
            GateScope::Sample(sid) => {
                self.sample_tree(sid)?;
                if let Some(p) = parent {
                    self.gate(sid, p)?;
                }
                let id = self.fresh_id("g");
                let node = GateNode { id: id.clone(), name: name.to_string(), parent: parent.map(String::from), gate: crate::gate::Gate { shape, negate, grid: None }, color: None };
                self.sample_gates.get_mut(sid).unwrap().push(SampleGate::new(node, true));
                self.invalidate_sample(sid);
                Ok(id)
            }
        }
    }

    /// Copy a template node into a sample (unless it already has that id or
    /// lacks the parent population).
    fn insert_template_copy(&mut self, sample_id: &str, node: &GateNode) {
        let copy = self.template_to_sample(sample_id, node);
        let Some(tree) = self.sample_gates.get_mut(sample_id) else { return };
        if tree.iter().any(|n| n.id == node.id) {
            return;
        }
        if let Some(p) = &node.parent {
            if !tree.iter().any(|n| &n.id == p) {
                return;
            }
        }
        tree.push(SampleGate::new(copy, false));
    }

    /// Update a gate of the "All Samples" group.
    /// `color`: `None` leaves the colour unchanged, `Some(None)` clears it.
    pub fn update_gate(&mut self, id: &str, shape: Option<GateShape>, name: Option<&str>, negate: Option<bool>, color: Option<Option<String>>) -> Result<()> {
        self.update_gate_in(&GateScope::default(), id, shape, name, negate, color)
    }

    /// Update a gate through a group or on one sample.
    ///
    /// * Group scope: the template is updated; every member sample's copy
    ///   gets the new name and colour, and the new shape / negation unless the
    ///   copy is tailored.
    /// * Sample scope: only that sample's copy changes; a change of shape,
    ///   negation or name marks it tailored.
    pub fn update_gate_in(&mut self, scope: &GateScope, id: &str, shape: Option<GateShape>, name: Option<&str>, negate: Option<bool>, color: Option<Option<String>>) -> Result<()> {
        if let Some(s) = &shape {
            validate_shape(s)?;
        }
        match scope {
            GateScope::Group(gid) => {
                let gi = self.group_index(gid)?;
                let g = self.groups[gi].gates.iter_mut().find(|g| g.id == id).ok_or_else(|| Error::GateNotFound(id.into()))?;
                apply_patch(g, shape.clone(), name, negate, color.clone());
                for sid in self.groups[gi].sample_ids.clone() {
                    let mut s = shape.clone();
                    if let Some(sh) = s.as_mut() {
                        self.shape_to_sample(&sid, sh);
                    }
                    let Some(n) = self.sample_gates.get_mut(&sid).and_then(|t| t.iter_mut().find(|n| n.id == id)) else { continue };
                    if n.tailored {
                        apply_patch(&mut n.node, None, name, None, color.clone());
                    } else {
                        apply_patch(&mut n.node, s, name, negate, color.clone());
                    }
                    self.invalidate_sample(&sid);
                }
            }
            GateScope::Sample(sid) => {
                let tree = self.sample_gates.get_mut(sid).ok_or_else(|| Error::SampleNotFound(sid.clone()))?;
                let n = tree.iter_mut().find(|n| n.id == id).ok_or_else(|| Error::GateNotFound(id.into()))?;
                if shape.is_some() || negate.is_some_and(|v| v != n.gate.negate) || name.is_some_and(|v| v != n.name) {
                    n.tailored = true;
                }
                apply_patch(&mut n.node, shape, name, negate, color);
                self.invalidate_sample(sid);
            }
        }
        Ok(())
    }

    /// Remove a gate of the "All Samples" group from the template and from
    /// every sample, with all descendants. Returns removed ids.
    pub fn remove_gate(&mut self, id: &str) -> Result<Vec<String>> {
        self.remove_gate_in(&GateScope::default(), id)
    }

    /// Remove a gate (and its descendants) from a group template and all of
    /// its member samples, or from one sample only. Returns the removed ids.
    pub fn remove_gate_in(&mut self, scope: &GateScope, id: &str) -> Result<Vec<String>> {
        let mut removed: HashSet<String> = HashSet::new();
        match scope {
            GateScope::Group(gid) => {
                let gi = self.group_index(gid)?;
                self.group_gate(gid, id)?;
                let doomed = subtree(&self.groups[gi].gates, id);
                self.groups[gi].gates.retain(|g| !doomed.contains(&g.id));
                removed.extend(doomed);
                for sid in self.groups[gi].sample_ids.clone() {
                    if let Some(tree) = self.sample_gates.get_mut(&sid) {
                        if tree.iter().any(|n| n.id == id) {
                            let doomed = subtree(tree, id);
                            tree.retain(|g| !doomed.contains(&g.id));
                            removed.extend(doomed);
                        }
                    }
                    self.invalidate_sample(&sid);
                }
            }
            GateScope::Sample(sid) => {
                self.gate(sid, id)?;
                let tree = self.sample_gates.get_mut(sid).unwrap();
                let doomed = subtree(tree, id);
                tree.retain(|g| !doomed.contains(&g.id));
                removed.extend(doomed);
                self.invalidate_sample(sid);
            }
        }
        Ok(removed.into_iter().collect())
    }

    /// Push a sample's (tailored) gate to a group: the template takes the
    /// sample's gate (re-expressed in the default transform space) and every
    /// member's copy is reset to it (no longer tailored). If the group does not
    /// have the gate yet, it is inserted (its parent must be in the template).
    pub fn copy_gate_to_group(&mut self, gate_id: &str, from_sample: &str, group_id: &str) -> Result<()> {
        let gi = self.group_index(group_id)?;
        let mut node = self.gate(from_sample, gate_id)?.clone();
        self.shape_to_default(from_sample, &mut node.gate.shape);
        {
            let tmpl = &mut self.groups[gi].gates;
            match tmpl.iter_mut().find(|g| g.id == gate_id) {
                Some(t) => {
                    t.name = node.name.clone();
                    t.gate = node.gate.clone();
                    t.color = node.color.clone();
                }
                None => {
                    if let Some(p) = &node.parent {
                        if !tmpl.iter().any(|g| &g.id == p) {
                            return Err(Error::InvalidGate(format!("the parent population of {} is not in group {}", node.name, self.groups[gi].name)));
                        }
                    }
                    tmpl.push(node.clone());
                }
            }
        }
        let mut members = self.groups[gi].sample_ids.clone();
        if !members.iter().any(|s| s == from_sample) {
            members.push(from_sample.to_string());
        }
        for sid in members {
            let copy = self.template_to_sample(&sid, &node);
            let Some(tree) = self.sample_gates.get_mut(&sid) else { continue };
            match tree.iter_mut().find(|n| n.id == gate_id) {
                Some(n) => {
                    n.node.name = copy.name;
                    n.node.gate = copy.gate;
                    n.node.color = copy.color;
                    n.tailored = false;
                }
                None => {
                    if copy.parent.as_ref().is_none_or(|p| tree.iter().any(|n| &n.id == p)) {
                        tree.push(SampleGate::new(copy, false));
                    }
                }
            }
            self.invalidate_sample(&sid);
        }
        Ok(())
    }

    fn invalidate_sample(&mut self, sample_id: &str) {
        if let Some(l) = self.loaded.get_mut(sample_id) {
            l.membership.clear();
        }
    }

    /// Path of names from the root to a gate of a sample's tree, joined by `/`.
    pub fn gate_path(&self, sample_id: &str, id: &str) -> String {
        self.sample_gates.get(sample_id).map(|t| path_of(t, id)).unwrap_or_default()
    }

    /// Path of names from the root to a template gate of a group.
    pub fn group_gate_path(&self, group_id: &str, id: &str) -> String {
        self.group(group_id).map(|g| path_of(&g.gates, id)).unwrap_or_default()
    }

    /// Boolean membership vector for a population (gate id of the sample's
    /// tree), including parents.
    pub fn membership(&mut self, sample_id: &str, gate_id: &str) -> Result<Arc<Vec<bool>>> {
        if let Some(l) = self.loaded.get(sample_id) {
            if let Some(m) = l.membership.get(gate_id) {
                return Ok(m.clone());
            }
        }
        let node = self.gate(sample_id, gate_id)?.clone();
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
                // Any shape with a display grid is evaluated on snapped events
                // (set by the .wsp importer per gate type; see `Gate::grid`).
                if let Some(g) = node.gate.grid {
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
                let node = self.gate(sample_id, g)?;
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

    /// Stats for every population of the sample's tree (pre-order).
    pub fn all_population_stats(&mut self, sample_id: &str) -> Result<Vec<PopulationStats>> {
        let mut out = vec![self.population_stats(sample_id, None)?];
        for id in self.preorder(sample_id) {
            out.push(self.population_stats(sample_id, Some(&id))?);
        }
        Ok(out)
    }

    /// Gate ids of a sample's tree in depth-first pre-order.
    pub fn preorder(&self, sample_id: &str) -> Vec<String> {
        self.sample_gates.get(sample_id).map(|t| preorder_of(t)).unwrap_or_default()
    }

    /// Template gate ids of a group in depth-first pre-order.
    pub fn group_preorder(&self, group_id: &str) -> Vec<String> {
        self.group(group_id).map(|g| preorder_of(&g.gates)).unwrap_or_default()
    }

    pub fn channel_statistics(&mut self, sample_id: &str, gate_id: Option<&str>, channel: &str, compensated: bool) -> Result<ChannelStats> {
        let col = self.column(sample_id, channel, compensated)?;
        let mask = match gate_id {
            Some(g) => Some(self.membership(sample_id, g)?),
            None => None,
        };
        Ok(channel_stats(channel, &col, mask.as_deref().map(|v| v.as_slice())))
    }

    // ------------------------------------------------------- layouts & UI

    /// The named layouts of the layout editor.
    pub fn layouts(&self) -> &[Layout] {
        &self.layouts
    }

    /// Replace all layouts.
    pub fn set_layouts(&mut self, layouts: Vec<Layout>) {
        self.layouts = layouts;
    }

    /// The stored UI preferences (`null` when never set).
    pub fn ui_state(&self) -> &serde_json::Value {
        &self.ui_state
    }

    /// Replace the stored UI preferences.
    pub fn set_ui_state(&mut self, ui_state: serde_json::Value) {
        self.ui_state = ui_state;
    }

    // -------------------------------------------------------- serialisation

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parse a workspace document; version-1 files (one shared `gates` list)
    /// become an "All Samples" group whose gates are copied into every sample.
    pub fn from_json(json: &str) -> Result<Self> {
        let mut ws: Workspace = serde_json::from_str(json)?;
        ws.next_id = 1;
        ws.normalize();
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

fn dedup(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

fn apply_patch(g: &mut GateNode, shape: Option<GateShape>, name: Option<&str>, negate: Option<bool>, color: Option<Option<String>>) {
    if let Some(c) = color {
        g.color = c;
    }
    if let Some(s) = shape {
        g.gate.shape = s;
    }
    if let Some(n) = name {
        g.name = n.to_string();
    }
    if let Some(n) = negate {
        g.gate.negate = n;
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
