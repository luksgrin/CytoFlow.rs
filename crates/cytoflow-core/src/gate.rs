//! Gates and the gate hierarchy.
//!
//! Gate geometry is stored in *display space* (the transformed `[0,1]`-ish
//! scale of each dimension) so that a polygon drawn on screen has straight
//! edges on screen, matching the Gating-ML semantic that gates are evaluated
//! after transformation.

use serde::{Deserialize, Serialize};

/// One axis of a gate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateDim {
    /// FCS `$PnN` channel name.
    pub channel: String,
    /// Whether the compensated value of that channel is used.
    #[serde(default)]
    pub compensated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GateShape {
    /// 1-D or 2-D axis-aligned box. Missing bounds are open; each interval is
    /// half-open (`min ≤ x < max`) as in Gating-ML 2.0.
    Rectangle {
        dims: Vec<GateDim>,
        min: Vec<Option<f64>>,
        max: Vec<Option<f64>>,
    },
    Polygon {
        dims: [GateDim; 2],
        vertices: Vec<[f64; 2]>,
    },
    /// Rotated ellipse: centre, half-axes `a` (along `angle`) and `b`, angle in radians.
    Ellipse {
        dims: [GateDim; 2],
        center: [f64; 2],
        a: f64,
        b: f64,
        angle: f64,
    },
    /// Boolean combination of other gates (by id), evaluated on the full sample.
    Boolean {
        op: BoolOp,
        refs: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BoolOp {
    And,
    Or,
    Not,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Gate {
    pub shape: GateShape,
    /// Gating-ML quadrant / FlowJo "eventsInside=0" support: invert membership.
    #[serde(default)]
    pub negate: bool,
    /// Evaluate a polygon on events snapped to a `grid`-step display grid
    /// (each display coordinate rounded to the nearest multiple of `1/grid`).
    /// Set for polygons imported from FlowJo, whose `gateResolution` grid is
    /// applied to the events when membership is tested; `None` (the default)
    /// evaluates the exact values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<u32>,
}

/// Snap display-space values to the nearest multiple of `1/grid`.
pub fn snap_to_grid(values: &[f32], grid: u32) -> Vec<f32> {
    let g = grid as f64;
    values.iter().map(|&v| ((v as f64 * g).round() / g) as f32).collect()
}

/// A node of the population tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateNode {
    pub id: String,
    pub name: String,
    /// `None` for a root population (child of "All events").
    pub parent: Option<String>,
    pub gate: Gate,
    /// Display colour (CSS hex), optional.
    #[serde(default)]
    pub color: Option<String>,
}

impl GateShape {
    pub fn dims(&self) -> Vec<&GateDim> {
        match self {
            GateShape::Rectangle { dims, .. } => dims.iter().collect(),
            GateShape::Polygon { dims, .. } | GateShape::Ellipse { dims, .. } => dims.iter().collect(),
            GateShape::Boolean { .. } => vec![],
        }
    }

    pub fn dims_mut(&mut self) -> Vec<&mut GateDim> {
        match self {
            GateShape::Rectangle { dims, .. } => dims.iter_mut().collect(),
            GateShape::Polygon { dims, .. } | GateShape::Ellipse { dims, .. } => dims.iter_mut().collect(),
            GateShape::Boolean { .. } => vec![],
        }
    }

    /// Evaluate the gate on pre-transformed columns (`cols[k]` matches `dims()[k]`).
    pub fn evaluate(&self, cols: &[&[f32]], out: &mut [bool]) {
        match self {
            GateShape::Rectangle { min, max, .. } => {
                for v in out.iter_mut() {
                    *v = true;
                }
                for (k, col) in cols.iter().enumerate() {
                    // Bounds are compared in f64 (not rounded to f32) so that
                    // values just below a bound stay outside it.
                    let lo = min.get(k).copied().flatten();
                    let hi = max.get(k).copied().flatten();
                    for (i, &x) in col.iter().enumerate() {
                        let x = x as f64;
                        if let Some(lo) = lo {
                            if x < lo {
                                out[i] = false;
                                continue;
                            }
                        }
                        // Gating-ML 2.0 §5.1: an interval is half-open, `min ≤ x < max`.
                        if let Some(hi) = hi {
                            if x >= hi {
                                out[i] = false;
                            }
                        }
                    }
                }
            }
            GateShape::Polygon { vertices, .. } => {
                let (xs, ys) = (cols[0], cols[1]);
                let poly: Vec<(f64, f64)> = vertices.iter().map(|v| (v[0], v[1])).collect();
                for i in 0..out.len() {
                    out[i] = point_in_polygon(xs[i] as f64, ys[i] as f64, &poly);
                }
            }
            GateShape::Ellipse { center, a, b, angle, .. } => {
                let (xs, ys) = (cols[0], cols[1]);
                let (sin, cos) = angle.sin_cos();
                let (a2, b2) = (a * a, b * b);
                for i in 0..out.len() {
                    let dx = xs[i] as f64 - center[0];
                    let dy = ys[i] as f64 - center[1];
                    let u = dx * cos + dy * sin;
                    let v = -dx * sin + dy * cos;
                    out[i] = (u * u) / a2 + (v * v) / b2 <= 1.0;
                }
            }
            GateShape::Boolean { .. } => {
                // Boolean gates are evaluated by the workspace, which has the referenced memberships.
                for v in out.iter_mut() {
                    *v = false;
                }
            }
        }
    }

    /// Convert a Gating-ML ellipsoid (mean, covariance, distance²) to an ellipse.
    pub fn ellipse_from_covariance(
        dims: [GateDim; 2],
        mean: [f64; 2],
        cov: [[f64; 2]; 2],
        dist2: f64,
    ) -> GateShape {
        // Eigen-decomposition of the symmetric 2x2 covariance matrix.
        let (p, q, r) = (cov[0][0], cov[0][1], cov[1][1]);
        let tr = p + r;
        let det = p * r - q * q;
        let disc = ((tr * tr) / 4.0 - det).max(0.0).sqrt();
        let l1 = tr / 2.0 + disc;
        let l2 = tr / 2.0 - disc;
        let angle = if q.abs() > 1e-15 { (l1 - p).atan2(q) } else if p >= r { 0.0 } else { std::f64::consts::FRAC_PI_2 };
        GateShape::Ellipse {
            dims,
            center: mean,
            a: (l1.max(0.0) * dist2).sqrt(),
            b: (l2.max(0.0) * dist2).sqrt(),
            angle,
        }
    }

    /// Build an ellipse from two foci and a point on the edge (FlowJo `.wsp` style).
    pub fn ellipse_from_foci(dims: [GateDim; 2], f1: [f64; 2], f2: [f64; 2], edge: [f64; 2]) -> GateShape {
        let center = [(f1[0] + f2[0]) / 2.0, (f1[1] + f2[1]) / 2.0];
        let d = |p: [f64; 2], q: [f64; 2]| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt();
        let c = d(f1, f2) / 2.0;
        let a = (d(edge, f1) + d(edge, f2)) / 2.0;
        let b = (a * a - c * c).max(0.0).sqrt();
        let angle = (f2[1] - f1[1]).atan2(f2[0] - f1[0]);
        GateShape::Ellipse { dims, center, a, b: if b > 0.0 { b } else { a }, angle }
    }
}

/// Ray-casting point-in-polygon test (Gating-ML: boundary points are inside).
pub fn point_in_polygon(x: f64, y: f64, poly: &[(f64, f64)]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        // On-edge check.
        let cross = (x - xi) * (yj - yi) - (y - yi) * (xj - xi);
        if cross.abs() < 1e-12
            && x >= xi.min(xj) - 1e-12
            && x <= xi.max(xj) + 1e-12
            && y >= yi.min(yj) - 1e-12
            && y <= yi.max(yj) + 1e-12
        {
            return true;
        }
        if (yi > y) != (yj > y) {
            let xint = (xj - xi) * (y - yi) / (yj - yi) + xi;
            if x < xint {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_contains() {
        let sq = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        assert!(point_in_polygon(0.5, 0.5, &sq));
        assert!(point_in_polygon(1.0, 0.5, &sq));
        assert!(!point_in_polygon(1.5, 0.5, &sq));
    }

    #[test]
    fn ellipse_from_cov_roundtrip() {
        let d = || GateDim { channel: "x".into(), compensated: false };
        let s = GateShape::ellipse_from_covariance([d(), d()], [0.0, 0.0], [[62.5, 37.5], [37.5, 62.5]], 1.0);
        if let GateShape::Ellipse { a, b, angle, .. } = s {
            assert!((a - 10.0).abs() < 1e-9);
            assert!((b - 5.0).abs() < 1e-9);
            assert!((angle - std::f64::consts::FRAC_PI_4).abs() < 1e-9);
        } else {
            panic!()
        }
    }
}
