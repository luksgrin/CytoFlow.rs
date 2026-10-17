//! FlowJo polygons drawn on FlowJo `biex` axes, compared with the counts
//! FlowJo stored in the workspace.
//!
//! Fixture: the FlowSOM R package's example data (`inst/extdata/gating.wsp`,
//! written by FlowJo 10.5.3, and `inst/extdata/68983.fcs`; FlowSOM, GPL ≥ 2,
//! Van Gassen et al.). It is not committed: `scripts/fetch-testdata.sh`
//! downloads it into `testdata/large/flowsom/`. The workspace records the
//! FCS file as `lymphocytes.fcs`, so the test copies `68983.fcs` under that
//! name into a temporary directory.
//!
//! The `ab T cells` and `gd T cells` polygons are drawn on two biex axes
//! (width −100, 4.418539922 positive decades), so unlike rectangles their
//! membership depends on the exact shape of the biex scale: a wrong scale
//! bends the polygon edges differently in raw space.

use cytoflow_core::wsp::import_wsp;
use cytoflow_core::Transform;
use std::path::{Path, PathBuf};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/large/flowsom")
}

/// Import the workspace with `edit` applied to its XML text, and return
/// `(path, flowjo, ours)` for every population.
fn counts(edit: impl Fn(&str) -> String) -> Option<Vec<(String, usize, usize)>> {
    let dir = fixture_dir();
    if !dir.join("68983.fcs").exists() || !dir.join("gating.wsp").exists() {
        eprintln!("skipping: FlowSOM fixture not found in testdata/large/flowsom (run scripts/fetch-testdata.sh)");
        return None;
    }
    let tmp = std::env::temp_dir().join(format!("cytoflow-flowsom-{}-{:?}", std::process::id(), std::thread::current().id()));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::copy(dir.join("68983.fcs"), tmp.join("lymphocytes.fcs")).unwrap();
    let text = std::fs::read_to_string(dir.join("gating.wsp")).unwrap();
    std::fs::write(tmp.join("gating.wsp"), edit(&text)).unwrap();
    let mut imp = import_wsp(tmp.join("gating.wsp"), std::slice::from_ref(&tmp)).expect("import");
    assert!(imp.missing_files.is_empty(), "{:?}", imp.missing_files);
    let ws = &mut imp.workspace;
    let sid = ws.samples[0].id.clone();
    let gates: Vec<(String, String)> = ws.sample_tree(&sid).unwrap().iter().map(|g| (g.id.clone(), ws.gate_path(&sid, &g.id))).collect();
    let mut out = Vec::new();
    for ec in &imp.expected_counts {
        let (gid, _) = gates.iter().find(|(_, p)| *p == ec.gate_path).expect("population imported");
        out.push((ec.gate_path.clone(), ec.count, ws.count(&sid, Some(gid)).unwrap()));
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Some(out)
}

#[test]
fn flowsom_biex_polygons_match_flowjo() {
    let Some(rows) = counts(|s| s.to_string()) else { return };
    // The fixture really exercises biex polygons.
    let imp = import_wsp(fixture_dir().join("gating.wsp"), &[]).unwrap();
    let sid0 = imp.workspace.samples[0].id.clone();
    assert!(matches!(imp.workspace.transform_for(&sid0, "APC-A"), Transform::Biex { .. }));
    for (path, flowjo, ours) in &rows {
        println!("{path:50} flowjo={flowjo:6} ours={ours:6} diff={:+}", *ours as i64 - *flowjo as i64);
    }
    // Agreement to within 2 events everywhere. The root `Lymphocytes`
    // polygon (linear FSC/SSC, `gateResolution` 1024) is already 2 events
    // short and `T cells` 1 short; the biex polygons below them differ by
    // +1 (`ab T cells`, 9188) and -2 (`gd T cells`, 1470), i.e. at most 1-2
    // boundary events of their own.
    for (path, flowjo, ours) in &rows {
        assert!((*ours as i64 - *flowjo as i64).abs() <= 2, "{path}: flowjo {flowjo}, ours {ours}");
    }
}

/// The former logicle approximation of biex (`W = (M − log10(T/|w|))/2`,
/// i.e. W = 0.5 for width −100) changes the biex polygons' counts, so this
/// fixture does discriminate between scales.
#[test]
fn flowsom_biex_polygons_reject_old_logicle_approximation() {
    let old = |s: &str| {
        s.replace(
            r#"<transforms:biex transforms:length="256"  transforms:maxRange="262144"  transforms:neg="0"  transforms:width="-100"  transforms:pos="4.418539922" >"#,
            r#"<transforms:logicle transforms:length="256"  transforms:T="262144"  transforms:A="0"  transforms:W="0.5"  transforms:M="4.418539922" >"#,
        )
        .replace("</transforms:biex>", "</transforms:logicle>")
    };
    let Some(rows) = counts(old) else { return };
    for (path, flowjo, ours) in &rows {
        println!("[old approximation] {path:50} flowjo={flowjo:6} ours={ours:6} diff={:+}", *ours as i64 - *flowjo as i64);
    }
    // With W = 0.5 the `ab T cells` polygon picks up 7 extra events (and its
    // child `CD4 T cells` 5), well outside the exact biex's ±2.
    let worst = rows.iter().map(|(_, f, o)| (*o as i64 - *f as i64).abs()).max().unwrap();
    assert!(worst >= 5, "the old approximation should visibly miss the biex polygons (worst {worst})");
}

