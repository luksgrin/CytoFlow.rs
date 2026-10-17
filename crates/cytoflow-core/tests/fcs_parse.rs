//! FCS reader and workspace serialisation tests on the public fixtures.

use cytoflow_core::compensation::CompMatrix;
use cytoflow_core::gate::{BoolOp, GateDim, GateShape};
use cytoflow_core::{FcsFile, Workspace};
use std::path::{Path, PathBuf};

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata")
}

fn kw_usize(f: &FcsFile, k: &str) -> usize {
    f.keyword(k)
        .unwrap_or_else(|| panic!("missing {k}"))
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn fcs30_float_big_endian_with_spill() {
    let f = FcsFile::open(testdata().join("test_comp_example.fcs")).unwrap();
    assert!(f.version.starts_with("FCS3.0"), "{}", f.version);
    assert_eq!(f.keyword("$DATATYPE").map(str::trim), Some("F"));
    assert_eq!(f.keyword("$BYTEORD").map(str::trim), Some("4,3,2,1"));
    assert_eq!(f.event_count, kw_usize(&f, "$TOT"));
    assert_eq!(f.parameters.len(), kw_usize(&f, "$PAR"));
    assert_eq!(f.data.len(), f.parameters.len());
    assert!(f.data.iter().all(|c| c.len() == f.event_count));
    for (i, p) in f.parameters.iter().enumerate() {
        assert_eq!(
            Some(p.name.as_str()),
            f.keyword(&format!("$P{}N", i + 1)).map(str::trim)
        );
    }
    assert!(f.data.iter().flatten().all(|v| v.is_finite()));

    let spill = f.spillover.clone().expect("$SPILL / $SPILLOVER");
    let n = spill.channels.len();
    assert!(n >= 2);
    assert_eq!(spill.matrix.len(), n);
    assert!(spill.matrix.iter().all(|r| r.len() == n));
    for (i, c) in spill.channels.iter().enumerate() {
        assert!(
            f.channel_index(c).is_some(),
            "spillover channel {c} not in file"
        );
        assert!(
            (spill.matrix[i][i] - 1.0).abs() < 1e-9,
            "diagonal of {c} is {}",
            spill.matrix[i][i]
        );
    }

    let comp = CompMatrix::from(spill.clone());
    let cols = comp.apply(&f).unwrap();
    assert_eq!(cols.len(), f.data.len());
    assert!(cols.iter().flatten().all(|v| v.is_finite()));
    // Non-matrix channels are untouched; matrix channels change.
    for (i, p) in f.parameters.iter().enumerate() {
        if !spill.channels.contains(&p.name) {
            assert_eq!(cols[i], f.data[i], "{}", p.name);
        }
    }
    // This file's $SPILL happens to be the identity matrix, so compensation
    // must leave the data unchanged.
    assert!(
        comp.is_identity(),
        "fixture's SPILL is expected to be the identity"
    );
    for c in &spill.channels {
        let i = f.channel_index(c).unwrap();
        assert_eq!(cols[i], f.data[i], "{c}");
    }

    // A non-trivial matrix on the same file: finite, changed, and invertible.
    let mut spill = spill;
    for i in 0..n {
        for j in 0..n {
            if i != j {
                spill.matrix[i][j] = 0.01 * ((i * 7 + j * 3) % 5) as f64;
            }
        }
    }
    let comp = CompMatrix::from(spill.clone());
    let cols = comp.apply(&f).unwrap();
    assert!(cols.iter().flatten().all(|v| v.is_finite()));
    assert!(spill.channels.iter().any(|c| {
        let i = f.channel_index(c).unwrap();
        cols[i] != f.data[i]
    }));
    // Re-applying the spillover to the compensated data restores the raw values.
    let idx: Vec<usize> = spill
        .channels
        .iter()
        .map(|c| f.channel_index(c).unwrap())
        .collect();
    for e in (0..f.event_count).step_by((f.event_count / 50).max(1)) {
        for (j, &cj) in idx.iter().enumerate() {
            let back: f64 = idx
                .iter()
                .enumerate()
                .map(|(k, &ck)| cols[ck][e] as f64 * spill.matrix[k][j])
                .sum();
            let raw = f.data[cj][e] as f64;
            assert!(
                (back - raw).abs() <= 1e-3 * raw.abs().max(1.0),
                "event {e} channel {j}: {back} vs {raw}"
            );
        }
    }
}

#[test]
fn fcs20_integer_with_log_amplification_and_gain() {
    let f = FcsFile::open(testdata().join("gate_ref/data1.fcs")).unwrap();
    assert_eq!(f.version, "FCS2.0");
    assert_eq!(f.event_count, 13367);
    assert_eq!(f.event_count, kw_usize(&f, "$TOT"));
    assert_eq!(
        f.channel_names(),
        ["FSC-H", "SSC-H", "FL1-H", "FL2-H", "FL3-H", "FL2-A", "FL4-H", "Time"]
    );
    assert!(
        f.spillover.is_none(),
        "data1.fcs carries no spillover keyword"
    );

    let max = |c: &str| {
        f.column(c)
            .unwrap()
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max)
    };
    let min = |c: &str| {
        f.column(c)
            .unwrap()
            .iter()
            .cloned()
            .fold(f32::INFINITY, f32::min)
    };
    // $P1G = 3.67: linear channel values are divided by the gain.
    assert!(max("FSC-H") <= 1023.0 / 3.67 + 1e-3);
    assert!(max("FSC-H") > 1023.0 / 3.67 * 0.5);
    // $P3E = 4,0: 4 decades, 10^(4·v/1024) with the offset 0 read as 1.
    assert!(min("FL1-H") >= 1.0 - 1e-6);
    assert!(max("FL1-H") <= 10f32.powf(4.0 * 1023.0 / 1024.0) * 1.0001);
    // Linear channel without gain stays in channel units.
    assert!(max("FL2-A") <= 1023.0 && max("FL2-A") > 100.0);
    assert!(f.data.iter().flatten().all(|v| v.is_finite()));
}

fn d(c: &str, comp: bool) -> GateDim {
    GateDim {
        channel: c.into(),
        compensated: comp,
    }
}

#[test]
fn workspace_json_roundtrip_with_every_gate_shape() {
    let mut ws = Workspace::new("roundtrip");
    let sid = ws
        .add_sample(testdata().join("gate_ref/data1.fcs"))
        .unwrap();
    // Gate coordinates are in display space: centre the gates on each
    // channel's median display value so that every gate is populated.
    let median = |ws: &mut Workspace, c: &str| -> f64 {
        let mut v: Vec<f32> = ws.transformed(&sid, c, false).unwrap().to_vec();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2] as f64
    };
    let (fsc, ssc, fl1, fl2) = (
        median(&mut ws, "FSC-H"),
        median(&mut ws, "SSC-H"),
        median(&mut ws, "FL1-H"),
        median(&mut ws, "FL2-H"),
    );
    let rect = ws
        .add_gate(
            "rect",
            None,
            GateShape::Rectangle {
                dims: vec![d("FSC-H", false), d("SSC-H", false)],
                min: vec![Some(fsc * 0.5), None],
                max: vec![Some(fsc * 1.5), Some(ssc * 1.5)],
            },
            false,
        )
        .unwrap();
    let range = ws
        .add_gate(
            "range",
            Some(&rect),
            GateShape::Rectangle {
                dims: vec![d("FL1-H", false)],
                min: vec![Some(fl1)],
                max: vec![None],
            },
            true,
        )
        .unwrap();
    let poly = ws
        .add_gate(
            "poly",
            Some(&rect),
            GateShape::Polygon {
                dims: [d("FL1-H", false), d("FL2-H", false)],
                vertices: vec![
                    [fl1 * 0.8, fl2 * 0.8],
                    [fl1 * 1.2, fl2 * 0.8],
                    [fl1 * 1.2, fl2 * 1.2],
                    [fl1 * 0.8, fl2 * 1.2],
                ],
            },
            false,
        )
        .unwrap();
    let ell = ws
        .add_gate(
            "ellipse",
            None,
            GateShape::Ellipse {
                dims: [d("FSC-H", false), d("SSC-H", false)],
                center: [fsc, ssc],
                a: fsc * 0.3,
                b: ssc * 0.3,
                angle: 0.4,
            },
            false,
        )
        .unwrap();
    let boolean = ws
        .add_gate(
            "or",
            None,
            GateShape::Boolean {
                op: BoolOp::Or,
                refs: vec![poly.clone(), ell.clone()],
            },
            false,
        )
        .unwrap();
    ws.gates
        .iter_mut()
        .find(|g| g.id == poly)
        .unwrap()
        .gate
        .grid = Some(256);

    let ids = [&rect, &range, &poly, &ell, &boolean];
    let counts: Vec<usize> = ids
        .iter()
        .map(|g| ws.count(&sid, Some(g)).unwrap())
        .collect();
    assert!(counts.iter().all(|c| *c > 0 && *c < 13367), "{counts:?}");

    let json = ws.to_json().unwrap();
    let mut back = Workspace::from_json(&json).unwrap();
    assert_eq!(back.name, ws.name);
    assert_eq!(back.gates, ws.gates);
    assert_eq!(back.samples, ws.samples);
    assert_eq!(back.channel_settings, ws.channel_settings);
    assert_eq!(back.compensation, ws.compensation);
    for g in &ws.gates {
        assert_eq!(back.gate_path(&g.id), ws.gate_path(&g.id));
    }
    assert!(back.reload_samples(None).is_empty());
    let back_counts: Vec<usize> = ids
        .iter()
        .map(|g| back.count(&sid, Some(g)).unwrap())
        .collect();
    assert_eq!(back_counts, counts);
    // New ids after a reload do not collide with existing ones.
    let new_id = back
        .add_gate(
            "extra",
            None,
            GateShape::Rectangle {
                dims: vec![d("Time", false)],
                min: vec![None],
                max: vec![Some(0.5)],
            },
            false,
        )
        .unwrap();
    assert!(!ids.iter().any(|g| **g == new_id));
}
