//! FlowJo rectangle gates drawn to the edge of the axis.
//!
//! The `Time` gate of `8_color_ICS_simple.wsp` has the same bounds in every
//! sample (`1.28 ≤ Time < 180.48` in `raw · gain` units, `FSC-A ≥ 30720`),
//! but each sample has its own linear Time axis `[minRange, maxRange]`:
//!
//! | sample | axis minimum | gate min (display cells of 1/256) | events below it | FlowJo counts them |
//! |--------|--------------|-----------------------------------|-----------------|--------------------|
//! | E01    | 1.27468      | 0.020                             | 2               | yes                |
//! | E03    | 0.85120      | 1.543                             | 1               | no                 |
//! | E05    | 1.02142      | 1.035                             | 357             | no                 |
//!
//! The importer therefore opens a bound that lies within the first or last
//! cell of the 256-step display grid. The other rules tried (all on every
//! rectangle population of the public `.wsp` fixtures) each break exact
//! counts: events snapped to the grid by rounding (E01 Time −4198), events
//! floored to bins with bounds floored/ceiled (E05 Time +17, reused quadrant
//! `good cells` +511), bounds only rounded to the grid (E03 Time −66), and an
//! edge cell of 1/128 (E05 Time +357). A 1/1024 edge cell gives the same
//! counts as 1/256 on these fixtures.

use cytoflow_core::wsp::import_wsp;
use cytoflow_core::workspace::GateScope;
use cytoflow_core::GateShape;
use std::path::{Path, PathBuf};

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata")
}

#[test]
fn time_gate_minimum_in_first_display_cell_is_open() {
    let large = testdata().join("large");
    if !large.join("101_DEN084Y5_15_E01_008_clean.fcs").exists() {
        eprintln!("skipping: 8-colour FCS files not found in testdata/large (run scripts/fetch-testdata.sh)");
        return;
    }
    let mut imp = import_wsp(testdata().join("wsp/8_color_ICS_simple.wsp"), &[large]).expect("import");
    let ws = &mut imp.workspace;
    let samples: Vec<(String, String)> = ws.samples.iter().map(|s| (s.id.clone(), s.name.clone())).collect();
    assert_eq!(samples.len(), 3);
    for (sid, name) in &samples {
        let gid = ws.preorder(sid).into_iter().find(|g| ws.gate_path(sid, g) == "Time").expect("Time gate");
        let GateShape::Rectangle { dims, min, max } = ws.gate(sid, &gid).unwrap().gate.shape.clone() else {
            panic!("Time is a rectangle");
        };
        assert_eq!(dims[0].channel, "Time");
        assert_eq!(dims[1].channel, "FSC-A");
        // The Time maximum lies beyond the axis and FSC-A's maximum on its
        // top edge: both are open in every sample. FSC-A's minimum
        // (30720 = 30/256 of the axis) stays a closed bound.
        assert_eq!(max, vec![None, None], "{name}");
        assert_eq!(min[1], Some(30720.0 / 262144.0), "{name}");
        let expected = imp.expected_counts.iter().find(|e| e.sample_name == *name && e.gate_path == "Time").unwrap().count;
        assert_eq!(ws.count(sid, Some(&gid)).unwrap(), expected, "{name} Time");

        // The exact display position of the 1.28 minimum on this sample's axis.
        let gain = match name {
            n if n.contains("E01") => 0.0101893145,
            n if n.contains("E03") => 0.0100853049,
            _ => 0.0102654811,
        };
        let lo = ws.transform_for(sid, "Time").apply(1.2799999714 / gain);
        if name.contains("E01") {
            assert!(lo > 0.0 && lo < 1.0 / 256.0, "{name}: {lo}");
            assert_eq!(min[0], None, "{name}: a minimum in the first cell is open");
            // With the exact bound restored, the 2 events below it drop out:
            // the edge rule is what makes E01 exact.
            let restored = GateShape::Rectangle { dims: dims.clone(), min: vec![Some(lo), min[1]], max: max.clone() };
            ws.update_gate_in(&GateScope::Sample(sid.clone()), &gid, Some(restored), None, None, None).unwrap();
            assert_eq!(ws.count(sid, Some(&gid)).unwrap(), expected - 2, "{name}");
        } else {
            assert!(lo > 1.0 / 256.0, "{name}: {lo}");
            let kept = min[0].expect("a minimum past the first cell is kept");
            assert!((kept - lo).abs() < 1e-12, "{name}: {kept} vs {lo}");
        }
    }
}
