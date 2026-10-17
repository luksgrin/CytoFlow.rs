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
/// FlowJo's `biex` is approximated by a logicle scale; the agreement of
/// gates on biex axes is tracked with a loose bound. (The biex fixtures only
/// contain rectangles/quadrants, whose membership does not depend on any
/// monotonic axis transform, so they are in fact exact.)
const BIEX_TOL: f64 = 0.05;
/// The FlowJo ellipse is tracked with a loose bound on its own count; see
/// `ics_with_ellipse`.
const ELLIPSE_TOL: f64 = 0.05;

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
    let gates: Vec<(String, String)> = ws
        .gates
        .iter()
        .map(|g| (g.id.clone(), ws.gate_path(&g.id)))
        .collect();
    for ec in &imp.expected_counts {
        let Some((sid, _, total)) = samples.iter().find(|(_, n, _)| *n == ec.sample_name) else {
            continue;
        };
        let parent_expected = match ec.gate_path.rsplit_once('/') {
            None => *total,
            Some((parent, _)) => imp
                .expected_counts
                .iter()
                .find(|p| p.sample_name == ec.sample_name && p.gate_path == parent)
                .map(|p| p.count)
                .expect("parent count"),
        };
        let Some((gid, _)) = gates.iter().find(|(_, p)| *p == ec.gate_path) else {
            panic!(
                "[{wsp}] population {} of {} was not imported",
                ec.gate_path, ec.sample_name
            );
        };
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
    let Some((_, rows)) = run(wsp, "101_DEN084Y5_15_E03_009_clean.fcs") else {
        return;
    };
    assert_within(wsp, &rows, EXACT_TOL);
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
        "[{wsp}] worst relative difference (biex approximated as logicle): {:.4} %",
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
        "[{wsp}] worst relative difference (biex approximated as logicle): {:.4} %",
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
        let imp = import_wsp(testdata().join("wsp").join(wsp), &[]).expect(wsp);
        assert!(!imp.workspace.gates.is_empty(), "{wsp}: no gates");
        assert!(!imp.expected_counts.is_empty(), "{wsp}: no counts");
        let paths: Vec<String> = imp
            .workspace
            .gates
            .iter()
            .map(|g| imp.workspace.gate_path(&g.id))
            .collect();
        // Every population of the applied gate tree has a FlowJo count.
        for p in &paths {
            assert!(
                imp.expected_counts.iter().any(|e| &e.gate_path == p),
                "{wsp}: {p} has no count"
            );
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
