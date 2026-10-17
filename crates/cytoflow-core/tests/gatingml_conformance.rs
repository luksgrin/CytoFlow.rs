//! Gating-ML 2.0 conformance: every gate in `testdata/gate_ref/gml/*.xml` is
//! evaluated on `data1.fcs` and compared, event by event, with the published
//! `Results_<GateId>.txt` membership files.

use cytoflow_core::gatingml::GatingMl;
use cytoflow_core::FcsFile;
use std::path::{Path, PathBuf};

fn gate_ref() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/gate_ref")
}

/// Gates that rely on features the core deliberately does not implement.
/// Each entry is `(gate id, reason)`; they are skipped explicitly, never silently.
const SKIPPED: &[(&str, &str)] = &[
    (
        "RatRange1",
        "fratio (new-dimension) dimensions are not supported",
    ),
    (
        "RatRange1a",
        "fratio (new-dimension) dimensions are not supported",
    ),
    (
        "RatRange2",
        "fratio (new-dimension) dimensions are not supported",
    ),
    ("ScaleRange2", "hyperlog transform is not supported"),
    ("ScaleRange2c", "hyperlog transform is not supported"),
    ("ScaleRange7c", "hyperlog transform is not supported"),
    ("ScalePar1", "hyperlog transform is not supported"),
    ("Ellipsoid3D", "3-D ellipsoids are not supported"),
];

/// Maximum number of mismatching events tolerated per gate. Zero: the
/// reference memberships are reproduced exactly.
const MAX_MISMATCHES: usize = 0;

fn read_truth(id: &str) -> Option<Vec<bool>> {
    let p = gate_ref().join("truth").join(format!("Results_{id}.txt"));
    let text = std::fs::read_to_string(p).ok()?;
    Some(
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| l == "1")
            .collect(),
    )
}

#[test]
fn gatingml_reference_gates_match_truth() {
    let fcs = FcsFile::open(gate_ref().join("data1.fcs")).expect("data1.fcs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(gate_ref().join("gml"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "xml"))
        .collect();
    files.sort();
    assert!(!files.is_empty());

    let mut checked = 0;
    let mut failures = Vec::new();
    let mut skipped = Vec::new();
    for file in &files {
        let fname = file.file_name().unwrap().to_string_lossy().to_string();
        let xml = std::fs::read_to_string(file).unwrap();
        let gml = GatingMl::parse(&xml).unwrap_or_else(|e| panic!("{fname}: parse error {e}"));
        // Every gate id declared in the file (including ones the parser skipped).
        let ids: Vec<String> = xml
            .split("gating:id=\"")
            .skip(1)
            .filter_map(|s| s.split('"').next().map(String::from))
            .collect();
        for id in ids {
            let Some(truth) = read_truth(&id) else {
                continue;
            };
            if let Some((_, why)) = SKIPPED.iter().find(|(s, _)| *s == id) {
                skipped.push(format!("{fname}:{id} ({why})"));
                continue;
            }
            assert_eq!(truth.len(), fcs.event_count, "{id}: truth length");
            let got = match gml.evaluate(&fcs, &id) {
                Ok(g) => g,
                Err(e) => {
                    failures.push(format!(
                        "{fname}:{id}: evaluation error {e} (warnings: {:?})",
                        gml.warnings
                    ));
                    continue;
                }
            };
            let mismatches = got.iter().zip(&truth).filter(|(a, b)| a != b).count();
            let n_in = truth.iter().filter(|b| **b).count();
            println!("{fname:55} {id:16} truth={n_in:6} mismatches={mismatches}");
            if mismatches > MAX_MISMATCHES {
                failures.push(format!(
                    "{fname}:{id}: {mismatches} mismatching events (truth has {n_in} inside)"
                ));
            }
            checked += 1;
        }
    }
    for s in &skipped {
        println!("skipped {s}");
    }
    for f in &failures {
        println!("FAIL {f}");
    }
    assert!(
        checked >= 60,
        "only {checked} gate/truth pairs were checked"
    );
    assert!(
        failures.is_empty(),
        "{} gate(s) disagree with the reference:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The truth files discriminate the spectrum-matrix orientation: with the
/// matrix read as rows = fluorochromes / columns = detectors the results are
/// exact (see above); transposing it must break agreement badly.
#[test]
fn spectrum_matrix_orientation_is_fluorochromes_by_detectors() {
    let fcs = FcsFile::open(gate_ref().join("data1.fcs")).unwrap();
    for (file, id) in [
        ("gml_matrix_rect3_gate.xml", "Rectangle3"),
        ("gml_matrix_poly4_gate.xml", "Polygon4"),
    ] {
        let xml = std::fs::read_to_string(gate_ref().join("gml").join(file)).unwrap();
        let mut gml = GatingMl::parse(&xml).unwrap();
        assert_eq!(gml.fluorochromes["MySpill"], ["FITC", "PE", "PerCP"]);
        assert_eq!(
            gml.matrices["MySpill"].channels,
            ["FL1-H", "FL2-H", "FL3-H"]
        );
        let truth = read_truth(id).unwrap();
        let count = |g: &GatingMl| {
            g.evaluate(&fcs, id)
                .unwrap()
                .iter()
                .zip(&truth)
                .filter(|(a, b)| a != b)
                .count()
        };
        assert_eq!(count(&gml), 0, "{id}");
        let m = gml.matrices.get_mut("MySpill").unwrap();
        let n = m.spillover.len();
        m.spillover = (0..n)
            .map(|i| (0..n).map(|j| m.spillover[j][i]).collect())
            .collect();
        assert!(
            count(&gml) > 100,
            "{id}: transposed matrix should not match the reference"
        );
    }
}
