//! The FlowJo biexponential scale against a lookup table exported by FlowJo
//! itself (`testdata/flowjo_xforms/`, from FlowKit's test data, BSD-3): for
//! width basis −7.943282, 1 extra negative decade, 4.418540 positive decades
//! and maxRange 262144.000029, channel `i` (of 4096) maps to the listed raw
//! value. Our analytic `Transform::Biex` must reproduce every entry.

use cytoflow_core::Transform;
use std::path::Path;

#[test]
fn biex_matches_flowjo_exported_table() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/flowjo_xforms/tr_biex_l256_w-7.943282_n1.000000_m4.418540_r262144.000029.csv");
    let text = std::fs::read_to_string(path).expect("table");
    let t = Transform::Biex { t: 262144.000029, w: -7.943282, pos: 4.418540, neg: 1.0 };
    let mut n = 0;
    let mut worst_rel = 0.0f64;
    let mut worst_abs = 0.0f64;
    for line in text.lines().skip(1) {
        let (i, v) = line.split_once(',').expect("row");
        let i: f64 = i.trim().parse().unwrap();
        let v: f64 = v.trim().parse().unwrap();
        let ours = t.invert(i / 4096.0);
        let err = (ours - v).abs();
        // The table is printed to 6 significant digits.
        assert!(err <= 6e-6 * v.abs() + 6e-4, "channel {i}: FlowJo {v}, ours {ours}");
        worst_rel = worst_rel.max(err / v.abs().max(1.0));
        worst_abs = worst_abs.max(err);
        // Forward direction: the display value of FlowJo's raw value is the channel.
        let y = t.apply(v);
        assert!((y * 4096.0 - i).abs() < 0.05, "channel {i}: forward {}", y * 4096.0);
        n += 1;
    }
    assert_eq!(n, 4096);
    println!("biex vs FlowJo table: {n} channels, worst abs {worst_abs:.3e}, worst rel {worst_rel:.3e}");
}
