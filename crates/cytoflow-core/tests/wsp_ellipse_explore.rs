//! Exploration of FlowJo's ellipse convention on logicle axes
//! (`8_color_ICS_with_ellipse.wsp`, population `.../CD3+/ellipse1`).
//!
//! The geometry (centre = midpoint of the foci, `a` from the first edge
//! vertex, `b = √(a² − c²)`, vertices = display·256) is exact on linear axes
//! (`tests/wsp_ellipse.rs`), but on these logicle axes it undercounts FlowJo
//! by 127 / 122 / 224 events (E03 / E05 / E01). The ignored tests below print
//! the count of each candidate display convention; run them with
//!
//! ```text
//! cargo test --release -p cytoflow-core --test wsp_ellipse_explore -- --ignored --nocapture
//! ```
//!
//! Findings (see the table printed by `ellipse_display_variants`):
//! * no discrete convention (FlowJo biex with the CytoML parameter mapping,
//!   4096/256-channel tables, rounding/flooring of the events, bin-aligned
//!   logicle zero, axis spanning the data range, polygonal ellipses) closes
//!   the gap; most make it larger;
//! * the deficit is captured by a one-parameter family — a display offset of
//!   ≈ −1.2 grid units, `M` ≈ 4.397 instead of 4.4185, `W` ≈ 1.025, or a biex
//!   width basis ≈ −110 — each of which brings all three samples to within
//!   ~20 events, but none of these values follows from the workspace;
//! * about 10–15 events of each deficit are inherited from the parents
//!   (`aAmine-` / `CD3+` polygons, ~0.07 % short): `parent_boundary_bound`.
//!
//! `polygon_vertices_lie_on_the_biex_grid` (not ignored) records a related
//! fact about the fixture: the polygon and rectangle coordinates of the `DEN`
//! group on the fluorescence channels sit on integer positions of a
//! 256-step FlowJo biex grid with width basis −10, 4.5 positive decades and
//! no negative decades — the display those gates were drawn on — not of
//! the `logicle` transform the workspace now records for those channels.

use cytoflow_core::transform::Biex;
use cytoflow_core::wsp::import_wsp;
use cytoflow_core::{GateShape, Transform};
use std::path::{Path, PathBuf};

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata")
}

const T: f64 = 262144.0;
const M: f64 = 4.418539922;

fn logicle(w: f64, m: f64, a: f64) -> Transform {
    Transform::Logicle { t: T, w, m, a }
}

type Map = Box<dyn Fn(f64) -> f64>;

/// One sample: the parent's events (compensated raw values of the two
/// ellipse dimensions), the ellipse in grid units and FlowJo's count.
struct Case {
    name: String,
    pts: Vec<(f64, f64)>,
    center: [f64; 2],
    a: f64,
    b: f64,
    angle: f64,
    expected: usize,
}

impl Case {
    fn inside(&self, gx: f64, gy: f64, scale: f64) -> bool {
        let (s, c) = self.angle.sin_cos();
        let (dx, dy) = (gx - self.center[0], gy - self.center[1]);
        let u = dx * c + dy * s;
        let v = -dx * s + dy * c;
        let (a, b) = (self.a * scale, self.b * scale);
        u * u / (a * a) + v * v / (b * b) <= 1.0
    }
}

fn cases() -> Option<Vec<Case>> {
    let large = testdata().join("large");
    if !large.join("101_DEN084Y5_15_E03_009_clean.fcs").exists() {
        eprintln!("skipping: FCS files missing (run scripts/fetch-testdata.sh)");
        return None;
    }
    let mut imp = import_wsp(testdata().join("wsp/8_color_ICS_with_ellipse.wsp"), &[large]).unwrap();
    let ws = &mut imp.workspace;
    let sids: Vec<(String, String)> = ws.samples.iter().map(|s| (s.id.clone(), s.name.clone())).collect();
    let mut out = Vec::new();
    for (sid, name) in &sids {
        let node = ws.sample_tree(sid).unwrap().iter().map(|g| g.node.clone()).find(|n| n.name == "ellipse1").unwrap();
        let expected = imp.expected_counts.iter().find(|e| e.sample_name == *name && e.gate_path.ends_with("ellipse1")).unwrap().count;
        let parent = ws.membership(sid, node.parent.as_deref().unwrap()).unwrap();
        let GateShape::Ellipse { dims, center, a, b, angle } = node.gate.shape.clone() else { panic!("not an ellipse") };
        let xs = ws.column(sid, &dims[0].channel, dims[0].compensated).unwrap();
        let ys = ws.column(sid, &dims[1].channel, dims[1].compensated).unwrap();
        let pts = (0..xs.len()).filter(|&i| parent[i]).map(|i| (xs[i] as f64, ys[i] as f64)).collect();
        out.push(Case { name: name[17..20].to_string(), pts, center: [center[0] * 256.0, center[1] * 256.0], a: a * 256.0, b: b * 256.0, angle, expected });
    }
    Some(out)
}

/// The ellipse count (minus FlowJo's) for each candidate mapping raw → grid.
#[test]
#[ignore]
fn ellipse_display_variants() {
    let Some(cases) = cases() else { return };
    let lg = logicle(1.0, M, 0.0);
    fn base(x: f64) -> f64 {
        256.0 * logicle(1.0, M, 0.0).apply(x)
    }
    let mut v: Vec<(String, Map)> = vec![
        ("logicle(W=1, M=4.4185) 256·y [current]".into(), Box::new(base) as Map),
        ("  round(256·y)".into(), Box::new(move |x| base(x).round())),
        ("  floor(256·y)".into(), Box::new(move |x| base(x).floor())),
        ("  floor(256·y) + 0.5".into(), Box::new(move |x| base(x).floor() + 0.5)),
        ("  255·y".into(), Box::new(move |x| base(x) * 255.0 / 256.0)),
        ("  257·y".into(), Box::new(move |x| base(x) * 257.0 / 256.0)),
    ];
    // 1. FlowJo biex with the CytoML mapping (widthBasis = −10^{2W}, pos = M, neg = A).
    for (wb, pos) in [(-100.0, M), (-10.0, M), (-100.0, 4.5), (-10.0, 4.5)] {
        let t = Transform::Biex { t: T, w: wb, pos, neg: 0.0 };
        v.push((format!("biex(wb={wb}, pos={pos:.4}) 256·y"), Box::new(move |x| 256.0 * t.apply(x))));
    }
    let bx = Transform::Biex { t: T, w: -100.0, pos: M, neg: 0.0 };
    let bx2 = bx.clone();
    v.push(("biex(-100) round(256·y)".into(), Box::new(move |x| (256.0 * bx2.apply(x)).round())));
    v.push(("biex(-100) 4096-channel floor /16".into(), Box::new(move |x| (4096.0 * bx.apply(x)).floor() / 16.0)));
    for n in [256.0, 1024.0] {
        let b = Biex::with_channels(T, -100.0, M, 0.0, n);
        v.push((format!("biex(-100) table of {n} channels"), Box::new(move |x| 256.0 * b.scale(x))));
    }
    // 2. Offsets of the logicle grid / bin-aligned zero.
    for d in [-1.5, -1.3, -1.2, -1.1, -1.0, -0.5, 0.5, 1.0] {
        v.push((format!("  256·y {d:+}"), Box::new(move |x| base(x) + d)));
    }
    let zero = ((1.0 / M) * 256.0f64).round() / 256.0;
    let a_bin = (M * zero - 1.0) / (1.0 - zero);
    let t = logicle(1.0, M, a_bin);
    v.push((format!("logicle zero on a 1/256 bin (A={a_bin:.5})"), Box::new(move |x| 256.0 * t.apply(x))));
    // Other logicle parameters (the one-parameter family that fits).
    for (w, m) in [(1.0, 4.39), (1.0, 4.395), (1.0, 4.4), (1.0, 4.41), (1.0, 250000f64.log10() - 1.0), (1.02, M), (1.03, M), (0.9, M), (1.1, M), (1.0, 4.5), (0.5, 4.5)] {
        let t = logicle(w, m, 0.0);
        v.push((format!("logicle(W={w}, M={m:.4})"), Box::new(move |x| 256.0 * t.apply(x))));
    }
    for wb in [-105.0, -110.0, -115.0] {
        let t = Transform::Biex { t: T, w: wb, pos: M, neg: 0.0 };
        v.push((format!("biex(wb={wb}, pos=M)"), Box::new(move |x| 256.0 * t.apply(x))));
    }
    // 2b. Grid spanning the display range of the data.
    let mut rows: Vec<(String, Vec<i64>)> = Vec::new();
    {
        let mut r = Vec::new();
        for c in &cases {
            let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
            for p in &c.pts {
                for (k, x) in [p.0, p.1].into_iter().enumerate() {
                    lo[k] = lo[k].min(lg.apply(x));
                    hi[k] = hi[k].max(lg.apply(x));
                }
            }
            let g = |k: usize, x: f64| 256.0 * (lg.apply(x) - lo[k]) / (hi[k] - lo[k]);
            r.push(c.pts.iter().filter(|p| c.inside(g(0, p.0), g(1, p.1), 1.0)).count() as i64 - c.expected as i64);
        }
        rows.push(("grid over the parent's display range".into(), r));
    }
    for (name, f) in &v {
        let r = cases.iter().map(|c| c.pts.iter().filter(|p| c.inside(f(p.0), f(p.1), 1.0)).count() as i64 - c.expected as i64).collect();
        rows.push((name.clone(), r));
    }
    // 3. Inscribed N-gons tested on snapped events.
    for nv in [32usize, 64, 256] {
        let r = cases
            .iter()
            .map(|c| {
                let (s, co) = c.angle.sin_cos();
                let poly: Vec<(f64, f64)> = (0..nv)
                    .map(|j| {
                        let t = std::f64::consts::TAU * j as f64 / nv as f64;
                        let (u, w) = (c.a * t.cos(), c.b * t.sin());
                        (c.center[0] + u * co - w * s, c.center[1] + u * s + w * co)
                    })
                    .collect();
                c.pts.iter().filter(|p| cytoflow_core::gate::point_in_polygon(base(p.0).round(), base(p.1).round(), &poly)).count() as i64 - c.expected as i64
            })
            .collect();
        rows.push((format!("{nv}-gon, round(256·y)"), r));
    }
    for sc in [1.01, 1.02, 1.03] {
        let r = cases.iter().map(|c| c.pts.iter().filter(|p| c.inside(base(p.0), base(p.1), sc)).count() as i64 - c.expected as i64).collect();
        rows.push((format!("ellipse scaled ×{sc}"), r));
    }
    println!("FlowJo: {:?}", cases.iter().map(|c| (c.name.clone(), c.expected)).collect::<Vec<_>>());
    println!("{:<44} {:>6} {:>6} {:>6}", "variant (ours − FlowJo)", cases[0].name, cases[1].name, cases[2].name);
    for (n, r) in rows {
        println!("{n:<44} {:>6} {:>6} {:>6}", r[0], r[1], r[2]);
    }
}

/// How much of the deficit comes from the parents: the ellipse counted on a
/// parent whose `aAmine-` and `CD3+` polygons are evaluated on unsnapped
/// events and pushed out by `r` grid units (`CD3+` reaches FlowJo's count
/// at r ≈ 0.2–0.3, where the ellipse gains only 10–15 events).
#[test]
#[ignore]
fn parent_boundary_bound() {
    let large = testdata().join("large");
    if !large.join("101_DEN084Y5_15_E03_009_clean.fcs").exists() {
        return;
    }
    let mut imp = import_wsp(testdata().join("wsp/8_color_ICS_with_ellipse.wsp"), &[large]).unwrap();
    let ws = &mut imp.workspace;
    let sids: Vec<(String, String)> = ws.samples.iter().map(|s| (s.id.clone(), s.name.clone())).collect();
    for (sid, sname) in &sids {
        let tree: Vec<_> = ws.sample_tree(sid).unwrap().iter().map(|g| g.node.clone()).collect();
        let find = |nm: &str| tree.iter().find(|n| n.name == nm).unwrap().clone();
        let expected = |suffix: &str| imp.expected_counts.iter().find(|e| e.sample_name == *sname && e.gate_path.ends_with(suffix)).unwrap().count as i64;
        let singlets = ws.membership(sid, &find("Singlets").id).unwrap();
        let ell = find("ellipse1");
        let in_ellipse = {
            let dims: Vec<_> = ell.gate.shape.dims().into_iter().cloned().collect();
            let cols: Vec<_> = dims.iter().map(|d| ws.transformed(sid, &d.channel, d.compensated).unwrap()).collect();
            let mut out = vec![false; cols[0].len()];
            ell.gate.shape.evaluate(&[&cols[0], &cols[1]], &mut out);
            out
        };
        let mut line = format!("{}:", &sname[17..20]);
        for r in [0.0, 0.25, 0.5, 1.0] {
            let mut par = (*singlets).clone();
            for nm in ["aAmine-", "CD3+"] {
                let g = find(nm);
                let GateShape::Polygon { dims, vertices } = &g.gate.shape else { panic!() };
                let cols: Vec<_> = dims.iter().map(|d| ws.transformed(sid, &d.channel, d.compensated).unwrap()).collect();
                let n = vertices.len() as f64;
                let (cx, cy) = (vertices.iter().map(|v| v[0]).sum::<f64>() / n, vertices.iter().map(|v| v[1]).sum::<f64>() / n);
                let poly: Vec<(f64, f64)> = vertices
                    .iter()
                    .map(|v| {
                        let (dx, dy) = (v[0] - cx, v[1] - cy);
                        let l = dx.hypot(dy);
                        (v[0] + dx / l * r / 256.0, v[1] + dy / l * r / 256.0)
                    })
                    .collect();
                for i in 0..par.len() {
                    par[i] = par[i] && cytoflow_core::gate::point_in_polygon(cols[0][i] as f64, cols[1][i] as f64, &poly);
                }
            }
            let cd3 = par.iter().filter(|&&b| b).count() as i64 - expected("aAmine-/CD3+");
            let e = (0..par.len()).filter(|&i| par[i] && in_ellipse[i]).count() as i64 - expected("ellipse1");
            line += &format!("  r={r}: ellipse {e:+} (CD3+ {cd3:+})");
        }
        println!("{line}");
    }
}

/// The `DEN` polygons and rectangles were drawn on FlowJo's default biex
/// display (width basis −10, 4.5 decades, no negative decades): `256·biex(x)`
/// of their fluorescence coordinates is an integer to 10⁻³, while under the
/// recorded `logicle` (W = 1, M = 4.4185) it is not.
#[test]
fn polygon_vertices_lie_on_the_biex_grid() {
    let text = std::fs::read_to_string(testdata().join("wsp/8_color_ICS_with_ellipse.wsp")).unwrap();
    let root = cytoflow_core::xml::parse(&text).unwrap();
    let den = root.descendants("GroupNode").into_iter().find(|g| g.attr("name") == Some("DEN")).expect("DEN group");
    let mut values = Vec::new();
    for poly in den.descendants("PolygonGate") {
        let dims: Vec<String> = poly.children_named("dimension").filter_map(|d| d.child("fcs-dimension").and_then(|f| f.attr("name")).map(String::from)).collect();
        for v in poly.children_named("vertex") {
            for (k, c) in v.children_named("coordinate").enumerate() {
                if dims.get(k).is_some_and(|d| d.starts_with("Comp-")) {
                    values.push(c.attr_f64("value").unwrap());
                }
            }
        }
    }
    for rect in den.descendants("RectangleGate") {
        for d in rect.children_named("dimension") {
            if d.child("fcs-dimension").and_then(|f| f.attr("name")).is_some_and(|n| n.starts_with("Comp-")) {
                // Bounds dragged past the top of the axis are off-grid.
                values.extend(["min", "max"].iter().filter_map(|b| d.attr_f64(b)).filter(|v| *v < T));
            }
        }
    }
    assert!(values.len() >= 40, "{} coordinates", values.len());
    let on_grid = |t: &Transform| values.iter().filter(|&&x| ((256.0 * t.apply(x)) - (256.0 * t.apply(x)).round()).abs() < 1e-3).count();
    let biex = on_grid(&Transform::Biex { t: T, w: -10.0, pos: 4.5, neg: 0.0 });
    let lg = on_grid(&logicle(1.0, M, 0.0));
    println!("{} coordinates: {biex} on the biex(−10, 4.5) grid, {lg} on the logicle(1, 4.4185) grid", values.len());
    assert!(biex as f64 >= 0.8 * values.len() as f64, "{biex} of {}", values.len());
    assert!(lg as f64 <= 0.1 * values.len() as f64, "{lg} of {}", values.len());
}
