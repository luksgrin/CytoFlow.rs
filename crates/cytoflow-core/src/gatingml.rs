//! Gating-ML 2.0 reader (ISAC standard) with a standalone evaluator, used for
//! conformance testing and for importing Gating-ML files.

use crate::compensation::CompMatrix;
use crate::error::{Error, Result};
use crate::fcs::FcsFile;
use crate::gate::{BoolOp, GateDim, GateShape};
use crate::transform::Transform;
use crate::xml::{self, Element};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct GmlDim {
    pub channel: String,
    pub transform_ref: Option<String>,
    /// `None`/`uncompensated`, `FCS`, or a spectrum-matrix id.
    pub comp_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GmlGate {
    pub id: String,
    pub parent: Option<String>,
    pub shape: GateShape,
    pub dims: Vec<GmlDim>,
}

#[derive(Debug, Default, Clone)]
pub struct GatingMl {
    pub gates: Vec<GmlGate>,
    pub transforms: HashMap<String, Transform>,
    pub matrices: HashMap<String, CompMatrix>,
    /// Fluorochrome names of each spectrum matrix, in row order. A gate
    /// dimension compensated with matrix `id` may name either a detector or
    /// one of these fluorochromes; fluorochrome `i` is read from the
    /// compensated column of detector `i`.
    pub fluorochromes: HashMap<String, Vec<String>>,
    pub warnings: Vec<String>,
}

/// Gate elements anywhere below `e` (some writers wrap gates in extra
/// container elements); gates nested inside other gates are not descended into.
fn gate_elements(e: &Element) -> Vec<&Element> {
    let mut out = Vec::new();
    for c in &e.children {
        if matches!(c.local(), "RectangleGate" | "PolygonGate" | "EllipsoidGate" | "BooleanGate" | "QuadrantGate") {
            out.push(c);
        } else if !matches!(c.local(), "transformation" | "spectrumMatrix") {
            out.extend(gate_elements(c));
        }
    }
    out
}

fn dim_of(e: &Element) -> Result<GmlDim> {
    let fcs_dim = e.child("fcs-dimension").ok_or_else(|| Error::Unsupported("gate dimension without fcs-dimension (ratio/new-dimension)".into()))?;
    let channel = fcs_dim.attr("name").ok_or_else(|| Error::Xml("fcs-dimension without name".into()))?.to_string();
    let comp_ref = e.attr("compensation-ref").map(String::from).filter(|s| s != "uncompensated");
    Ok(GmlDim { channel, transform_ref: e.attr("transformation-ref").map(String::from), comp_ref })
}

fn gate_dim(d: &GmlDim) -> GateDim {
    GateDim { channel: d.channel.clone(), compensated: d.comp_ref.is_some() }
}

fn coords(v: &Element) -> Vec<f64> {
    v.children_named("coordinate").filter_map(|c| c.attr_f64("value")).collect()
}

impl GatingMl {
    pub fn parse(xml_text: &str) -> Result<Self> {
        let root = xml::parse(xml_text)?;
        let mut g = GatingMl::default();

        for t in root.children_named("transformation") {
            let id = t.attr("id").unwrap_or_default().to_string();
            let Some(def) = t.children.first() else { continue };
            let tr = match def.local() {
                "flin" => Transform::Linear { t: def.attr_f64("T").unwrap_or(1.0), a: def.attr_f64("A").unwrap_or(0.0) },
                "flog" => Transform::Log { t: def.attr_f64("T").unwrap_or(1.0), m: def.attr_f64("M").unwrap_or(4.5) },
                "fasinh" => Transform::Asinh { t: def.attr_f64("T").unwrap_or(1.0), m: def.attr_f64("M").unwrap_or(4.5), a: def.attr_f64("A").unwrap_or(0.0) },
                "logicle" => Transform::Logicle { t: def.attr_f64("T").unwrap_or(1.0), w: def.attr_f64("W").unwrap_or(0.5), m: def.attr_f64("M").unwrap_or(4.5), a: def.attr_f64("A").unwrap_or(0.0) },
                other => {
                    g.warnings.push(format!("transformation {id}: unsupported type {other}"));
                    continue;
                }
            };
            g.transforms.insert(id, tr);
        }

        for m in root.children_named("spectrumMatrix") {
            let id = m.attr("id").unwrap_or_default().to_string();
            let fluoro: Vec<String> = m.child("fluorochromes").map(|f| f.children_named("fcs-dimension").filter_map(|d| d.attr("name").map(String::from)).collect()).unwrap_or_default();
            let det: Vec<String> = m.child("detectors").map(|f| f.children_named("fcs-dimension").filter_map(|d| d.attr("name").map(String::from)).collect()).unwrap_or_default();
            let mut rows: Vec<Vec<f64>> = m.children_named("spectrum").map(|r| r.children_named("coefficient").filter_map(|c| c.attr_f64("value")).collect()).collect();
            if fluoro.len() != det.len() || rows.len() != fluoro.len() {
                g.warnings.push(format!("spectrumMatrix {id}: non-square matrix, skipped"));
                continue;
            }
            if m.attr("matrix-inverted-already").is_some_and(|v| v == "true") {
                match crate::compensation::invert(&rows) {
                    Some(inv) => rows = inv,
                    None => {
                        g.warnings.push(format!("spectrumMatrix {id}: singular"));
                        continue;
                    }
                }
            }
            // Gating-ML: rows are fluorochromes, columns are detectors, i.e. the
            // same orientation as the FCS $SPILLOVER keyword, so
            // `compensated = raw · S⁻¹` (verified against the gml_matrix_* truth).
            g.fluorochromes.insert(id.clone(), fluoro);
            g.matrices.insert(id, CompMatrix { channels: det, spillover: rows });
        }

        for e in gate_elements(&root) {
            let id = e.attr("id").unwrap_or_default().to_string();
            let parent = e.attr("parent_id").map(String::from);
            match e.local() {
                "RectangleGate" => {
                    let dims_e: Vec<&Element> = e.children_named("dimension").collect();
                    let dims: Vec<GmlDim> = match dims_e.iter().map(|d| dim_of(d)).collect::<Result<_>>() {
                        Ok(d) => d,
                        Err(err) => {
                            g.warnings.push(format!("gate {id}: {err}"));
                            continue;
                        }
                    };
                    let min = dims_e.iter().map(|d| d.attr_f64("min")).collect();
                    let max = dims_e.iter().map(|d| d.attr_f64("max")).collect();
                    let shape = GateShape::Rectangle { dims: dims.iter().map(gate_dim).collect(), min, max };
                    g.gates.push(GmlGate { id, parent, shape, dims });
                }
                "PolygonGate" => {
                    let dims: Vec<GmlDim> = match e.children_named("dimension").map(dim_of).collect::<Result<_>>() {
                        Ok(d) => d,
                        Err(err) => {
                            g.warnings.push(format!("gate {id}: {err}"));
                            continue;
                        }
                    };
                    if dims.len() != 2 {
                        g.warnings.push(format!("gate {id}: polygon needs 2 dims"));
                        continue;
                    }
                    let vertices: Vec<[f64; 2]> = e.children_named("vertex").map(coords).filter(|c| c.len() == 2).map(|c| [c[0], c[1]]).collect();
                    let shape = GateShape::Polygon { dims: [gate_dim(&dims[0]), gate_dim(&dims[1])], vertices };
                    g.gates.push(GmlGate { id, parent, shape, dims });
                }
                "EllipsoidGate" => {
                    let dims: Vec<GmlDim> = match e.children_named("dimension").map(dim_of).collect::<Result<_>>() {
                        Ok(d) => d,
                        Err(err) => {
                            g.warnings.push(format!("gate {id}: {err}"));
                            continue;
                        }
                    };
                    if dims.len() != 2 {
                        g.warnings.push(format!("gate {id}: only 2-D ellipsoids are supported"));
                        continue;
                    }
                    let mean = e.child("mean").map(coords).unwrap_or_default();
                    let cov: Vec<Vec<f64>> = e.child("covarianceMatrix").map(|c| c.children_named("row").map(|r| r.children_named("entry").filter_map(|x| x.attr_f64("value")).collect()).collect()).unwrap_or_default();
                    let d2 = e.child("distanceSquare").and_then(|d| d.attr_f64("value")).unwrap_or(1.0);
                    if mean.len() != 2 || cov.len() != 2 || cov.iter().any(|r| r.len() != 2) {
                        g.warnings.push(format!("gate {id}: malformed ellipsoid"));
                        continue;
                    }
                    let shape = GateShape::ellipse_from_covariance([gate_dim(&dims[0]), gate_dim(&dims[1])], [mean[0], mean[1]], [[cov[0][0], cov[0][1]], [cov[1][0], cov[1][1]]], d2);
                    g.gates.push(GmlGate { id, parent, shape, dims });
                }
                "BooleanGate" => {
                    let Some(op_e) = e.children.iter().find(|c| matches!(c.local(), "and" | "or" | "not")) else { continue };
                    let op = match op_e.local() {
                        "and" => BoolOp::And,
                        "or" => BoolOp::Or,
                        _ => BoolOp::Not,
                    };
                    let mut refs = Vec::new();
                    let mut neg_refs = Vec::new();
                    for r in op_e.children_named("gateReference") {
                        if let Some(id) = r.attr("ref") {
                            if r.attr("use-as-complement").is_some_and(|v| v == "true") {
                                neg_refs.push(id.to_string());
                            } else {
                                refs.push(id.to_string());
                            }
                        }
                    }
                    // Complemented references become synthetic NOT gates.
                    for nid in neg_refs {
                        let synth = format!("__not_{nid}_{id}");
                        g.gates.push(GmlGate { id: synth.clone(), parent: None, shape: GateShape::Boolean { op: BoolOp::Not, refs: vec![nid] }, dims: vec![] });
                        refs.push(synth);
                    }
                    g.gates.push(GmlGate { id, parent, shape: GateShape::Boolean { op, refs }, dims: vec![] });
                }
                "QuadrantGate" => {
                    // Dividers define cut points; each quadrant picks an interval per divider.
                    struct Divider { id: String, dim: GmlDim, values: Vec<f64> }
                    let mut dividers: Vec<Divider> = Vec::new();
                    for d in e.children_named("divider") {
                        let did = d.attr("id").unwrap_or_default().to_string();
                        let dim = match dim_of(d) {
                            Ok(x) => x,
                            Err(err) => {
                                g.warnings.push(format!("quadrant {id}: {err}"));
                                continue;
                            }
                        };
                        let mut values: Vec<f64> = d.children_named("value").filter_map(|v| v.text.trim().parse().ok()).collect();
                        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        dividers.push(Divider { id: did, dim, values });
                    }
                    for q in e.children_named("Quadrant") {
                        let qid = q.attr("id").unwrap_or_default().to_string();
                        let mut dims = Vec::new();
                        let mut min = Vec::new();
                        let mut max = Vec::new();
                        for p in q.children_named("position") {
                            let dref = p.attr("divider_ref").unwrap_or_default();
                            let loc = p.attr_f64("location").unwrap_or(0.0);
                            let Some(div) = dividers.iter().find(|d| d.id == dref) else { continue };
                            let below = div.values.iter().filter(|v| **v <= loc).count();
                            let lo = if below == 0 { None } else { Some(div.values[below - 1]) };
                            let hi = div.values.get(below).copied();
                            dims.push(div.dim.clone());
                            min.push(lo);
                            max.push(hi);
                        }
                        let shape = GateShape::Rectangle { dims: dims.iter().map(gate_dim).collect(), min, max };
                        g.gates.push(GmlGate { id: qid, parent: parent.clone(), shape, dims });
                    }
                }
                _ => {}
            }
        }
        Ok(g)
    }

    pub fn gate(&self, id: &str) -> Option<&GmlGate> {
        self.gates.iter().find(|g| g.id == id)
    }

    /// Column for a dimension: compensated per `comp_ref`, then transformed per `transform_ref`.
    fn dim_column(&self, fcs: &FcsFile, d: &GmlDim, comp_cache: &mut HashMap<String, Vec<Vec<f32>>>) -> Result<Vec<f32>> {
        // `FCS` means the file's own spillover keyword. A file without one
        // (e.g. the FCS 2.0 `data1.fcs` reference file) is used uncompensated,
        // which is what the Gating-ML reference results expect.
        let comp_ref = match d.comp_ref.as_deref() {
            Some("FCS") if fcs.spillover.is_none() => None,
            other => other,
        };
        let raw: Vec<f32> = match comp_ref {
            None => {
                let idx = fcs.channel_index(&d.channel).ok_or_else(|| Error::ChannelNotFound(d.channel.clone()))?;
                fcs.data[idx].clone()
            }
            Some(r) => {
                let m = if r == "FCS" {
                    fcs.spillover.clone().map(CompMatrix::from).ok_or_else(|| Error::Compensation("FCS file has no spillover".into()))?
                } else {
                    self.matrices.get(r).cloned().ok_or_else(|| Error::Compensation(format!("unknown matrix {r}")))?
                };
                // A detector name, or a fluorochrome name mapped to its detector.
                let idx = match fcs.channel_index(&d.channel) {
                    Some(i) => i,
                    None => {
                        let k = self
                            .fluorochromes
                            .get(r)
                            .and_then(|f| f.iter().position(|n| *n == d.channel))
                            .ok_or_else(|| Error::ChannelNotFound(d.channel.clone()))?;
                        fcs.channel_index(&m.channels[k]).ok_or_else(|| Error::ChannelNotFound(m.channels[k].clone()))?
                    }
                };
                if !comp_cache.contains_key(r) {
                    comp_cache.insert(r.to_string(), m.apply(fcs)?);
                }
                comp_cache[r][idx].clone()
            }
        };
        match &d.transform_ref {
            None => Ok(raw),
            Some(t) => {
                let tr = self.transforms.get(t).ok_or_else(|| Error::Workspace(format!("unknown transformation {t}")))?;
                let mut out = Vec::new();
                tr.apply_slice(&raw, &mut out);
                Ok(out)
            }
        }
    }

    /// Evaluate a gate (including parents) on an FCS file.
    pub fn evaluate(&self, fcs: &FcsFile, id: &str) -> Result<Vec<bool>> {
        let mut comp_cache = HashMap::new();
        let mut memo = HashMap::new();
        self.eval_inner(fcs, id, &mut comp_cache, &mut memo)
    }

    fn eval_inner(&self, fcs: &FcsFile, id: &str, comp_cache: &mut HashMap<String, Vec<Vec<f32>>>, memo: &mut HashMap<String, Vec<bool>>) -> Result<Vec<bool>> {
        if let Some(m) = memo.get(id) {
            return Ok(m.clone());
        }
        let gate = self.gate(id).ok_or_else(|| Error::GateNotFound(id.into()))?;
        let n = fcs.event_count;
        let mut out = vec![false; n];
        match &gate.shape {
            GateShape::Boolean { op, refs } => {
                let members: Vec<Vec<bool>> = refs.iter().map(|r| self.eval_inner(fcs, r, comp_cache, memo)).collect::<Result<_>>()?;
                match op {
                    BoolOp::And => {
                        out.iter_mut().for_each(|v| *v = !members.is_empty());
                        for m in &members {
                            for i in 0..n {
                                out[i] &= m[i];
                            }
                        }
                    }
                    BoolOp::Or => {
                        for m in &members {
                            for i in 0..n {
                                out[i] |= m[i];
                            }
                        }
                    }
                    BoolOp::Not => {
                        if let Some(m) = members.first() {
                            for i in 0..n {
                                out[i] = !m[i];
                            }
                        }
                    }
                }
            }
            shape => {
                let cols: Vec<Vec<f32>> = gate.dims.iter().map(|d| self.dim_column(fcs, d, comp_cache)).collect::<Result<_>>()?;
                let refs: Vec<&[f32]> = cols.iter().map(|c| c.as_slice()).collect();
                shape.evaluate(&refs, &mut out);
            }
        }
        if let Some(p) = &gate.parent {
            let pm = self.eval_inner(fcs, p, comp_cache, memo)?;
            for i in 0..n {
                out[i] &= pm[i];
            }
        }
        memo.insert(id.to_string(), out.clone());
        Ok(out)
    }
}
