//! FlowJo ellipse (`gating:EllipsoidGate` with `foci` / `edge` vertices)
//! convention, pinned on linear axes where the display mapping is exact.
//!
//! Fixture (FlowKit, BSD-3): `testdata/wsp/single_ellipse_51_events.wsp`
//! with `data_set_simple_line_100.fcs`, 100 events evenly spaced on the line
//! `y = 128` (display units) from x = 64 to x = 128. FlowJo counts 51 of them
//! inside the ellipse, so the boundary has to fall between event 50
//! (x = 96.32) and event 51 (x = 96.97).

use cytoflow_core::wsp::import_wsp;
use cytoflow_core::GateShape;
use std::path::Path;

#[test]
fn flowjo_ellipse_on_linear_axes_is_exact() {
    let wsp = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/wsp/single_ellipse_51_events.wsp");
    let mut imp = import_wsp(&wsp, &[]).expect("import");
    assert!(imp.missing_files.is_empty());
    let ws = &mut imp.workspace;
    let sid = ws.samples[0].id.clone();
    let gid = ws.gates[0].id.clone();
    let GateShape::Ellipse { center, a, b, .. } = ws.gates[0].gate.shape.clone() else { panic!("not an ellipse") };
    // Centre = midpoint of the foci = midpoint of the edge vertices;
    // semi-major axis = half the distance between the first two edge
    // vertices (39.58 grid units), not `gating:distance / 2` (44.6).
    assert!((center[0] * 256.0 - 78.5).abs() < 1e-9 && (center[1] * 256.0 - 125.5).abs() < 1e-9);
    assert!((a * 256.0 - 39.579).abs() < 1e-3, "a = {}", a * 256.0);
    assert!((b * 256.0 - 17.357).abs() < 1e-3, "b = {}", b * 256.0);
    assert_eq!(imp.expected_counts[0].count, 51);
    assert_eq!(ws.count(&sid, Some(&gid)).unwrap(), 51);
}
