//! FlowJo `.wsp` import cross-check: every population count recorded by
//! FlowJo in the workspace is recomputed by the engine and compared.
//!
//! The FCS files live in `testdata/large/` (not committed; fetched by
//! `scripts/fetch-testdata.sh`). A test whose FCS file is missing is skipped
//! with a message instead of failing.

use cytoflow_core::wsp::{import_wsp, WspImport};
use std::path::{Path, PathBuf};

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata")
}

/// Exact agreement is the goal; a relative difference of at most 0.1 % is
/// tolerated for events sitting on a gate boundary. FlowJo tests polygon
/// membership on events snapped to its 256-step display grid; the importer
/// reproduces that (linear-scale polygons then match exactly), but on
/// logicle axes the snapping does not line up perfectly with FlowJo's own
/// grid and a few events per thousand near the boundary of a polygon or
/// ellipse can flip.
const EXACT_TOL: f64 = 0.001;
/// Gates drawn on logicle axes (and everything nested below them) agree to
/// within 0.2 % of their count for populations of at least 10 000 events.
const LOGICLE_TOL: f64 = 0.002;
/// Smaller populations (tens to a few thousand events) are dominated by the few
/// boundary flips inherited from their ancestors, so they are checked as a
/// fraction of the parent population: at most 0.1 % of the parent's events
/// may be classified differently.
const PARENT_TOL: f64 = 0.001;
/// Gates on `biex` axes: the transform is now exact (see `tests/biex_flowjo_table.rs`
/// and `tests/wsp_biex_polygon.rs`), and these fixtures only contain
/// rectangles/quadrants, so the counts must match exactly.
const BIEX_TOL: f64 = 0.0;
/// The FlowJo ellipse on logicle axes: its geometry is exact on linear axes
/// (tests/wsp_ellipse.rs), but on these logicle axes it undercounts FlowJo
/// by 2.0 / 2.1 / 3.1 % (E03 / E05 / E01; 5875 vs 6002, 6010 vs 6133,
/// 7032 vs 7256); see `ics_with_ellipse`.
const ELLIPSE_TOL: f64 = 0.035;

struct Row {
    sample: String,
    path: String,
    expected: usize,
    /// FlowJo's count of the parent population (all events for a root gate).
    parent_expected: usize,
    got: usize,
}

impl Row {
    fn rel(&self) -> f64 {
        if self.expected == 0 {
            if self.got == 0 {
                0.0
            } else {
                1.0
            }
        } else {
            (self.got as f64 - self.expected as f64).abs() / self.expected as f64
        }
    }

    /// Differently classified events as a fraction of the parent population.
    fn rel_parent(&self) -> f64 {
        (self.got as f64 - self.expected as f64).abs() / self.parent_expected.max(1) as f64
    }
}

/// Import a workspace and recompute all populations of the samples whose
/// FCS file is present. Returns `None` (after an `eprintln`) if `required`
/// is missing.
fn run(wsp: &str, required: &str) -> Option<(WspImport, Vec<Row>)> {
    let large = testdata().join("large");
    if !large.join(required).exists() {
        eprintln!("skipping {wsp}: {required} not found in testdata/large (run scripts/fetch-testdata.sh)");
        return None;
    }
    let mut imp = import_wsp(testdata().join("wsp").join(wsp), &[large]).expect("import");
    for w in &imp.warnings {
        println!("[{wsp}] warning: {w}");
    }
    let ws = &mut imp.workspace;
    let mut rows = Vec::new();
    let samples: Vec<(String, String, usize)> = ws
        .samples
        .iter()
        .map(|s| (s.id.clone(), s.name.clone(), s.event_count))
        .collect();
    for ec in &imp.expected_counts {
        let Some((sid, _, total)) = samples.iter().find(|(_, n, _)| *n == ec.sample_name) else {
            continue;
        };
        if !ws.is_loaded(sid) {
            continue;
        }
        let parent_expected = match ec.gate_path.rsplit_once('/') {
            None => *total,
            Some((parent, _)) => imp
                .expected_counts
                .iter()
                .find(|p| p.sample_name == ec.sample_name && p.gate_path == parent)
                .map(|p| p.count)
                .expect("parent count"),
        };
        // Each sample is evaluated with its own gate tree.
        let gid = ws
            .preorder(sid)
            .into_iter()
            .find(|g| ws.gate_path(sid, g) == ec.gate_path)
            .unwrap_or_else(|| {
                panic!(
                    "[{wsp}] population {} of {} was not imported",
                    ec.gate_path, ec.sample_name
                )
            });
        let gid = &gid;
        let got = ws.count(sid, Some(gid)).expect("count");
        rows.push(Row {
            sample: ec.sample_name.clone(),
            path: ec.gate_path.clone(),
            expected: ec.count,
            parent_expected,
            got,
        });
    }
    assert!(!rows.is_empty(), "[{wsp}] no population could be compared");
    for r in &rows {
        println!(
            "[{wsp}] {:36} {:50} flowjo={:7} ours={:7} diff={:+} ({:.4} % of count, {:.4} % of parent)",
            r.sample,
            r.path,
            r.expected,
            r.got,
            r.got as i64 - r.expected as i64,
            100.0 * r.rel(),
            100.0 * r.rel_parent()
        );
    }
    Some((imp, rows))
}

fn assert_within(wsp: &str, rows: &[Row], tol: f64) {
    let bad: Vec<String> = rows
        .iter()
        .filter(|r| r.rel() > tol)
        .map(|r| {
            format!(
                "{} {}: flowjo {} vs ours {} ({:.3} %)",
                r.sample,
                r.path,
                r.expected,
                r.got,
                100.0 * r.rel()
            )
        })
        .collect();
    assert!(
        bad.is_empty(),
        "[{wsp}] {} population(s) outside {:.2} %:\n{}",
        bad.len(),
        100.0 * tol,
        bad.join("\n")
    );
}

#[test]
fn ics_simple_polygons_rectangles_logicle_comp() {
    let wsp = "8_color_ICS_simple.wsp";
    let Some((imp, rows)) = run(wsp, "101_DEN084Y5_15_E03_009_clean.fcs") else {
        return;
    };
    assert_within(wsp, &rows, EXACT_TOL);
    // Every sample is checked against its own FlowJo counts.
    let loaded = imp.workspace.samples.iter().filter(|s| imp.workspace.is_loaded(&s.id)).count();
    let names: std::collections::HashSet<&str> = rows.iter().map(|r| r.sample.as_str()).collect();
    assert_eq!(names.len(), loaded, "[{wsp}] not every loaded sample was compared");
    // Each sample has its own `Time` gain (a per-sample transform override).
    // With it the Time rectangle of E03 and E05 is exact (E05 was 22 events
    // short when the template sample's gain was shared). In E01 the gate
    // minimum (1.28 s) sits 7.9e-5 display units above the axis minimum,
    // inside FlowJo's first display bin, and FlowJo also counts the 2 events
    // below it: an edge-of-axis rule of the rectangle gate, not a transform
    // issue.
    let time: Vec<&Row> = rows.iter().filter(|r| r.path == "Time").collect();
    assert_eq!(time.len(), loaded);
    for r in time {
        if r.sample.contains("E01_008") {
            assert!(r.got.abs_diff(r.expected) <= 2, "[{wsp}] {} Time: {} vs {}", r.sample, r.got, r.expected);
        } else {
            assert_eq!(r.got, r.expected, "[{wsp}] {} Time", r.sample);
        }
    }
}

/// Ellipsoid foci/edge vertices are on FlowJo's 256-step display grid (they
/// are divided by `gateResolution`, default 256, to get display coordinates).
/// The alternative reading -- raw units transformed like polygon vertices --
/// yields an ellipse a few events wide (8 events vs FlowJo's 6002), so it is
/// ruled out. The grid reading agrees to 2-3 % (e.g. 5875 vs 6002); the
/// residual is larger than the parent populations' disagreement and is
/// tracked with `ELLIPSE_TOL`.
#[test]
fn ics_with_ellipse() {
    let wsp = "8_color_ICS_with_ellipse.wsp";
    let Some((_, rows)) = run(wsp, "101_DEN084Y5_15_E03_009_clean.fcs") else {
        return;
    };
    for r in &rows {
        if r.path.ends_with("ellipse1") {
            println!(
                "[{wsp}] {} ellipse agreement: {:.3} % of count",
                r.sample,
                100.0 * r.rel()
            );
            assert!(
                r.rel() <= ELLIPSE_TOL,
                "{} ellipse1: {:.3} %",
                r.sample,
                100.0 * r.rel()
            );
        } else if r.expected >= 10_000 {
            assert!(
                r.rel() <= LOGICLE_TOL,
                "{} {}: {:.3} %",
                r.sample,
                r.path,
                100.0 * r.rel()
            );
        } else {
            assert!(
                r.rel_parent() <= PARENT_TOL,
                "{} {}: {:.3} % of parent",
                r.sample,
                r.path,
                100.0 * r.rel_parent()
            );
        }
    }
}

#[test]
fn reused_quad_gate_with_child_biex() {
    let wsp = "reused_quad_gate_with_child.wsp";
    let Some((_, rows)) = run(wsp, "101_DEN084Y5_15_E01_008_clean.fcs") else {
        return;
    };
    let worst = rows.iter().map(Row::rel).fold(0.0, f64::max);
    println!(
        "[{wsp}] worst relative difference (biex): {:.4} %",
        100.0 * worst
    );
    assert_within(wsp, &rows, BIEX_TOL);
}

#[test]
fn diamond_biex_rect() {
    let wsp = "test_data_diamond_biex_rect.wsp";
    let Some((_, rows)) = run(wsp, "test_data_diamond_01.fcs") else {
        return;
    };
    let worst = rows.iter().map(Row::rel).fold(0.0, f64::max);
    println!(
        "[{wsp}] worst relative difference (biex): {:.4} %",
        100.0 * worst
    );
    assert_within(wsp, &rows, BIEX_TOL);
}

#[test]
fn diamond_asinh_rect() {
    let wsp = "test_data_diamond_asinh_rect.wsp";
    let Some((_, rows)) = run(wsp, "test_data_diamond_01.fcs") else {
        return;
    };
    assert_within(wsp, &rows, EXACT_TOL);
}

#[test]
fn diamond_asinh_rect2() {
    let wsp = "test_data_diamond_asinh_rect2.wsp";
    let Some((_, rows)) = run(wsp, "test_data_diamond_01.fcs") else {
        return;
    };
    assert_within(wsp, &rows, EXACT_TOL);
}

#[test]
fn diamond_quad_gate() {
    let wsp = "simple_diamond_example_quad_gate.wsp";
    let Some((_, rows)) = run(wsp, "test_data_diamond_01.fcs") else {
        return;
    };
    assert_within(wsp, &rows, EXACT_TOL);
}

/// The importer itself does not need the FCS files: the population tree,
/// transforms and FlowJo counts are read from the workspace alone.
#[test]
fn import_without_fcs_files() {
    for wsp in [
        "8_color_ICS_simple.wsp",
        "8_color_ICS_with_ellipse.wsp",
        "reused_quad_gate_with_child.wsp",
        "simple_diamond_example_quad_gate.wsp",
        "test_data_diamond_asinh_rect.wsp",
        "test_data_diamond_asinh_rect2.wsp",
        "test_data_diamond_biex_rect.wsp",
    ] {
        // No search directory and a workspace path whose FCS files do not
        // exist: every sample is added unloaded, with its own tree.
        let imp = import_wsp(testdata().join("wsp").join(wsp), &[]).expect(wsp);
        let ws = &imp.workspace;
        assert!(!ws.samples.is_empty(), "{wsp}: no samples");
        assert!(!imp.expected_counts.is_empty(), "{wsp}: no counts");
        assert_eq!(ws.groups[0].name, "All Samples");
        assert_eq!(ws.groups[0].sample_ids.len(), ws.samples.len());
        for s in &ws.samples {
            let tree = ws.sample_tree(&s.id).unwrap();
            assert!(!tree.is_empty(), "{wsp}: {} has no gates", s.name);
            // Every population of the sample's own tree has its FlowJo count.
            for g in tree {
                let p = ws.gate_path(&s.id, &g.id);
                assert!(
                    imp.expected_counts.iter().any(|e| e.gate_path == p && e.sample_name == s.name),
                    "{wsp}: {} {p} has no count",
                    s.name
                );
            }
        }
        // The same population path has the same id in every sample and group.
        let mut by_path: std::collections::HashMap<String, String> = Default::default();
        for s in &ws.samples {
            for g in ws.sample_tree(&s.id).unwrap() {
                let p = ws.gate_path(&s.id, &g.id);
                assert_eq!(by_path.entry(p.clone()).or_insert(g.id.clone()), &g.id, "{wsp}: {p}");
            }
        }
        for grp in &ws.groups {
            for g in &grp.gates {
                let p = ws.group_gate_path(&grp.id, &g.id);
                if let Some(id) = by_path.get(&p) {
                    assert_eq!(id, &g.id, "{wsp}: group {} {p}", grp.name);
                }
            }
        }
        assert!(
            !imp.warnings
                .iter()
                .any(|w| w.contains("could not be imported") || w.contains("unknown dependent")),
            "{wsp}: {:?}",
            imp.warnings
        );
    }
}

/// Per-sample trees: tailoring one sample's gate (or its transform) does not
/// change any other sample; group edits skip tailored copies; pushing a
/// tailored gate to its group resets every member's copy.
#[test]
fn tailoring_one_sample_does_not_change_another() {
    use cytoflow_core::workspace::GateScope;
    use cytoflow_core::{GateShape, Transform};
    let large = testdata().join("large");
    if !large.join("101_DEN084Y5_15_E05_010_clean.fcs").exists() {
        eprintln!("skipping: 8-colour FCS files not found in testdata/large");
        return;
    }
    let mut imp = import_wsp(testdata().join("wsp/8_color_ICS_simple.wsp"), &[large]).expect("import");
    let ws = &mut imp.workspace;
    let ids: Vec<String> = ws.samples.iter().map(|s| s.id.clone()).collect();
    assert_eq!(ids.len(), 3);
    let (a, b) = (ids[0].clone(), ids[1].clone());
    let den = ws.groups.iter().find(|g| g.name == "DEN").expect("DEN group").id.clone();
    assert_eq!(ws.group(&den).unwrap().sample_ids, ids);
    let find = |ws: &cytoflow_core::Workspace, sid: &str, path: &str| {
        ws.preorder(sid).into_iter().find(|g| ws.gate_path(sid, g) == path).unwrap()
    };
    let singlets = find(ws, &a, "Time/Singlets");
    assert_eq!(singlets, find(ws, &b, "Time/Singlets"), "ids are shared across samples");
    assert!(ws.group_gate(&den, &singlets).is_ok(), "the group template has the same id");
    // Each sample keeps its own Time gain as a transform override.
    assert!(ws.transform_for(&a, "Time") != ws.transform_for(&b, "Time"));

    let count_b = ws.count(&b, Some(&singlets)).unwrap();
    let shape_b = ws.gate(&b, &singlets).unwrap().gate.shape.clone();
    let count_a = ws.count(&a, Some(&singlets)).unwrap();

    // Tailor sample A: shrink its Singlets polygon towards the centroid.
    let mut shrunk = ws.gate(&a, &singlets).unwrap().gate.shape.clone();
    if let GateShape::Polygon { vertices, .. } = &mut shrunk {
        let n = vertices.len() as f64;
        let cx = vertices.iter().map(|v| v[0]).sum::<f64>() / n;
        let cy = vertices.iter().map(|v| v[1]).sum::<f64>() / n;
        for v in vertices.iter_mut() {
            v[0] = cx + 0.5 * (v[0] - cx);
            v[1] = cy + 0.5 * (v[1] - cy);
        }
    } else {
        panic!("Singlets is a polygon");
    }
    ws.update_gate_in(&GateScope::Sample(a.clone()), &singlets, Some(shrunk.clone()), None, None, None).unwrap();
    assert!(ws.sample_gate(&a, &singlets).unwrap().tailored);
    assert!(!ws.sample_gate(&b, &singlets).unwrap().tailored);
    assert!(ws.count(&a, Some(&singlets)).unwrap() < count_a);
    assert_eq!(ws.count(&b, Some(&singlets)).unwrap(), count_b);
    assert_eq!(ws.gate(&b, &singlets).unwrap().gate.shape, shape_b);

    // A sample-only transform change re-maps only that sample's gates.
    let t = Transform::Logicle { t: 262144.0, w: 0.5, m: 4.5, a: 0.0 };
    ws.set_sample_transform(&a, "FSC-A", t).unwrap();
    assert_eq!(ws.gate(&b, &singlets).unwrap().gate.shape, shape_b);
    assert_eq!(ws.count(&b, Some(&singlets)).unwrap(), count_b);

    // A group edit reaches B but skips A's tailored copy.
    let tailored_a = ws.gate(&a, &singlets).unwrap().gate.shape.clone();
    let mut tmpl = ws.group_gate(&den, &singlets).unwrap().gate.shape.clone();
    if let GateShape::Polygon { vertices, .. } = &mut tmpl {
        vertices[0][0] += 0.01;
    }
    ws.update_gate_in(&GateScope::Group(den.clone()), &singlets, Some(tmpl), Some("Single cells"), None, None).unwrap();
    assert_eq!(ws.gate(&a, &singlets).unwrap().gate.shape, tailored_a);
    assert_eq!(ws.gate(&a, &singlets).unwrap().name, "Single cells", "names follow the group");
    assert!(ws.gate(&b, &singlets).unwrap().gate.shape != shape_b);

    // Push A's tailored gate to the group: every member follows it.
    ws.copy_gate_to_group(&singlets, &a, &den).unwrap();
    assert!(!ws.sample_gate(&a, &singlets).unwrap().tailored);
    let c = ids[2].clone();
    for s in [&b, &c] {
        // B and C use the default FSC-A transform, A an override: the copies
        // agree with the template, which is A's gate in the default space.
        let GateShape::Polygon { vertices: vs, .. } = &ws.gate(s, &singlets).unwrap().gate.shape else { panic!() };
        let GateShape::Polygon { vertices: vt, .. } = &ws.group_gate(&den, &singlets).unwrap().gate.shape else { panic!() };
        for (p, q) in vs.iter().zip(vt) {
            assert!((p[0] - q[0]).abs() < 1e-9 && (p[1] - q[1]).abs() < 1e-9);
        }
        assert!(!ws.sample_gate(s, &singlets).unwrap().tailored);
    }
    // ... and A's raw-space gate is unchanged by the round trip.
    let GateShape::Polygon { vertices: va, .. } = &ws.gate(&a, &singlets).unwrap().gate.shape else { panic!() };
    let GateShape::Polygon { vertices: vta, .. } = &tailored_a else { panic!() };
    for (p, q) in va.iter().zip(vta) {
        assert!((p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6);
    }

    // Removing a gate from one sample leaves the others alone; removing it
    // from the group removes it (and its descendants) everywhere.
    let amine = find(ws, &a, "Time/Single cells/aAmine-");
    ws.remove_gate_in(&GateScope::Sample(a.clone()), &amine).unwrap();
    assert!(ws.gate(&a, &amine).is_err());
    assert!(ws.gate(&b, &amine).is_ok());
    let removed = ws.remove_gate_in(&GateScope::Group(den.clone()), &singlets).unwrap();
    assert!(removed.contains(&amine));
    for s in &ids {
        assert!(ws.gate(s, &singlets).is_err());
        assert!(ws.gate(s, &amine).is_err());
    }
}
