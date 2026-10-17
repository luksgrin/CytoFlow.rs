//! Axis transforms, following the Gating-ML 2.0 definitions so that every
//! transform maps the instrument range onto roughly `[0, 1]`.
//!
//! * `flin(x; T, A) = (x + A) / (T + A)`
//! * `flog(x; T, M) = log10(x / T) / M + 1`
//! * `fasinh(x; T, M, A) = (asinh(x·sinh(M·ln10)/T) + A·ln10) / ((M + A)·ln10)`
//! * `logicle(x; T, W, M, A)` — Parks, Roederer & Moore, Cytometry A 69A:541 (2006)

use serde::{Deserialize, Serialize};

const LN10: f64 = std::f64::consts::LN_10;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TransformKind {
    Linear,
    Log,
    Asinh,
    Logicle,
}

/// A fully parameterised transform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Transform {
    /// `[-A, T]` → `[0, 1]`
    Linear { t: f64, a: f64 },
    /// `[T·10^-M, T]` → `[0, 1]`
    Log { t: f64, m: f64 },
    /// hyperbolic arcsine with `M` positive decades and `A` negative decades.
    Asinh { t: f64, m: f64, a: f64 },
    /// biexponential: `T` top of scale, `W` linearisation width (decades),
    /// `M` total decades, `A` additional negative decades.
    Logicle { t: f64, w: f64, m: f64, a: f64 },
}

impl Default for Transform {
    fn default() -> Self {
        Transform::Linear { t: 262144.0, a: 0.0 }
    }
}

impl Transform {
    pub fn kind(&self) -> TransformKind {
        match self {
            Transform::Linear { .. } => TransformKind::Linear,
            Transform::Log { .. } => TransformKind::Log,
            Transform::Asinh { .. } => TransformKind::Asinh,
            Transform::Logicle { .. } => TransformKind::Logicle,
        }
    }

    /// Sensible defaults for a channel given its range and (optionally) data.
    pub fn default_for(kind: TransformKind, range: f64, data: Option<&[f32]>) -> Transform {
        let t = if range > 0.0 { range } else { 262144.0 };
        match kind {
            TransformKind::Linear => Transform::Linear { t, a: 0.0 },
            TransformKind::Log => Transform::Log { t, m: 4.5 },
            TransformKind::Asinh => Transform::Asinh { t, m: 4.5, a: 0.0 },
            TransformKind::Logicle => {
                let m = 4.5;
                let w = data.map(|d| estimate_logicle_width(d, t, m)).unwrap_or(0.5);
                Transform::Logicle { t, w, m, a: 0.0 }
            }
        }
    }

    /// Forward transform: raw value → display scale.
    pub fn apply(&self, x: f64) -> f64 {
        match *self {
            Transform::Linear { t, a } => (x + a) / (t + a),
            Transform::Log { t, m } => {
                if x <= 0.0 {
                    // Gating-ML leaves this undefined; clamp to the bottom of scale.
                    f64::NEG_INFINITY.max(-1.0)
                } else {
                    (x / t).log10() / m + 1.0
                }
            }
            Transform::Asinh { t, m, a } => {
                ((x * (m * LN10).sinh() / t).asinh() + a * LN10) / ((m + a) * LN10)
            }
            Transform::Logicle { t, w, m, a } => Logicle::new(t, w, m, a).scale(x),
        }
    }

    /// Inverse transform: display scale → raw value.
    pub fn invert(&self, y: f64) -> f64 {
        match *self {
            Transform::Linear { t, a } => y * (t + a) - a,
            Transform::Log { t, m } => t * 10f64.powf((y - 1.0) * m),
            Transform::Asinh { t, m, a } => {
                (y * (m + a) * LN10 - a * LN10).sinh() * t / (m * LN10).sinh()
            }
            Transform::Logicle { t, w, m, a } => Logicle::new(t, w, m, a).inverse(y),
        }
    }

    /// Transform a whole column. Uses a cached logicle solver.
    pub fn apply_slice(&self, xs: &[f32], out: &mut Vec<f32>) {
        out.clear();
        out.reserve(xs.len());
        match *self {
            Transform::Logicle { t, w, m, a } => {
                let lg = Logicle::new(t, w, m, a);
                out.extend(xs.iter().map(|&x| lg.scale(x as f64) as f32));
            }
            _ => out.extend(xs.iter().map(|&x| self.apply(x as f64) as f32)),
        }
    }

    /// Tick marks for an axis using this transform: `(position, label, major)`.
    pub fn ticks(&self) -> Vec<(f64, String, bool)> {
        match *self {
            Transform::Linear { t, a } => {
                let lo = -a;
                let hi = t;
                let span = hi - lo;
                let step = nice_step(span / 5.0);
                let mut v = (lo / step).ceil() * step;
                let mut out = Vec::new();
                while v <= hi + 1e-9 {
                    if v.abs() < step * 1e-9 {
                        v = 0.0;
                    }
                    let label = if step >= 1000.0 { format!("{}K", v / 1000.0) } else { format!("{v}") };
                    out.push((self.apply(v), label, true));
                    v += step;
                }
                out
            }
            _ => {
                // Decade ticks: 0, ±10^k, with minor ticks at 2..9 × 10^k.
                let top = match *self {
                    Transform::Log { t, .. } | Transform::Asinh { t, .. } | Transform::Logicle { t, .. } => t,
                    _ => 262144.0,
                };
                let bottom = self.invert(0.0);
                let mut out = Vec::new();
                let max_dec = top.log10().floor() as i32;
                let min_dec = if bottom <= 0.0 { 0 } else { bottom.log10().ceil() as i32 };
                for sign in [1.0, -1.0] {
                    for k in min_dec..=max_dec {
                        let base = 10f64.powi(k);
                        for mult in 1..10 {
                            let v = sign * mult as f64 * base;
                            if v > top || v < bottom {
                                continue;
                            }
                            let y = self.apply(v);
                            if !(0.0..=1.0).contains(&y) {
                                continue;
                            }
                            let major = mult == 1;
                            let label = if major { format_decade(v) } else { String::new() };
                            out.push((y, label, major));
                        }
                    }
                }
                if bottom < 0.0 {
                    out.push((self.apply(0.0), "0".into(), true));
                }
                out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                out
            }
        }
    }
}

fn nice_step(raw: f64) -> f64 {
    if raw <= 0.0 {
        return 1.0;
    }
    let exp = raw.log10().floor();
    let base = 10f64.powf(exp);
    let frac = raw / base;
    let nice = if frac < 1.5 { 1.0 } else if frac < 3.5 { 2.0 } else if frac < 7.5 { 5.0 } else { 10.0 };
    nice * base
}

fn format_decade(v: f64) -> String {
    let k = v.abs().log10().round() as i32;
    let sign = if v < 0.0 { "-" } else { "" };
    match k {
        0 => format!("{sign}1"),
        1 => format!("{sign}10"),
        2 => format!("{sign}100"),
        _ => format!("{sign}10^{k}"),
    }
}

/// Estimate the logicle width from the 5th percentile of the negative data,
/// as recommended by Parks et al. (2006): `W = (M - log10(T / |r|)) / 2`.
pub fn estimate_logicle_width(data: &[f32], t: f64, m: f64) -> f64 {
    let mut neg: Vec<f32> = data.iter().copied().filter(|v| *v < 0.0).collect();
    if neg.len() < 10 {
        return 0.5;
    }
    neg.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((neg.len() as f64) * 0.05).floor() as usize;
    let r = neg[idx.min(neg.len() - 1)].abs() as f64;
    if r <= 0.0 {
        return 0.5;
    }
    let w = (m - (t / r).log10()) / 2.0;
    w.clamp(0.0, m / 2.0)
}

/// The logicle (biexponential) scale.
///
/// Implemented from the published definition: the scale is the inverse of
/// `B(y) = a·e^{b·y} − c·e^{−d·y} − f`, extended symmetrically about the
/// linearisation point `x1` for negative values.
#[derive(Debug, Clone, Copy)]
pub struct Logicle {
    t: f64,
    w: f64,
    m: f64,
    a_add: f64,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    f: f64,
    x1: f64,
}

impl Logicle {
    pub fn new(t: f64, w: f64, m: f64, a_add: f64) -> Self {
        let t = if t > 0.0 { t } else { 262144.0 };
        let m = if m > 0.0 { m } else { 4.5 };
        let w = w.max(0.0).min(m / 2.0);
        let a_add = a_add.max(-w).min(m - 2.0 * w);

        let w_n = w / (m + a_add);
        let x2 = a_add / (m + a_add);
        let x1 = x2 + w_n;
        let x0 = x2 + 2.0 * w_n;
        let b = (m + a_add) * LN10;
        let d = solve_d(b, w_n);
        let c_a = (x0 * (b + d)).exp();
        let mf_a = (b * x1).exp() - c_a / (d * x1).exp();
        let a = t / ((b.exp() - mf_a) - c_a / d.exp());
        let c = c_a * a;
        let f = mf_a * a;
        Logicle { t, w, m, a_add, a, b, c, d, f, x1 }
    }

    pub fn params(&self) -> (f64, f64, f64, f64) {
        (self.t, self.w, self.m, self.a_add)
    }

    /// Display scale → raw.
    pub fn inverse(&self, y: f64) -> f64 {
        let negative = y < self.x1;
        let y = if negative { 2.0 * self.x1 - y } else { y };
        let v = self.a * (self.b * y).exp() - self.c * (-self.d * y).exp() - self.f;
        if negative { -v } else { v }
    }

    fn slope(&self, y: f64) -> f64 {
        let y = if y < self.x1 { 2.0 * self.x1 - y } else { y };
        self.a * self.b * (self.b * y).exp() + self.c * self.d * (-self.d * y).exp()
    }

    /// Raw → display scale (Newton iteration with bisection safeguard).
    pub fn scale(&self, x: f64) -> f64 {
        if !x.is_finite() {
            return if x.is_nan() { f64::NAN } else if x > 0.0 { 2.0 } else { -1.0 };
        }
        // Bracket.
        let (mut lo, mut hi) = (self.x1 - 1.0, 1.0);
        while self.inverse(lo) > x {
            lo -= 1.0;
            if lo < -50.0 {
                break;
            }
        }
        while self.inverse(hi) < x {
            hi += 1.0;
            if hi > 50.0 {
                break;
            }
        }
        let mut y = 0.5 * (lo + hi);
        for _ in 0..60 {
            let fy = self.inverse(y) - x;
            if fy.abs() < 1e-12 * self.t.max(1.0) {
                return y;
            }
            if fy > 0.0 { hi = y } else { lo = y }
            let s = self.slope(y);
            let mut next = y - fy / s;
            if !(next > lo && next < hi) || !next.is_finite() {
                next = 0.5 * (lo + hi);
            }
            if (next - y).abs() < 1e-13 {
                return next;
            }
            y = next;
        }
        y
    }
}

/// Solve `2·(ln d − ln b) + w·(b + d) = 0` for `d` (Parks et al. appendix).
fn solve_d(b: f64, w: f64) -> f64 {
    if w <= 0.0 {
        return b;
    }
    let tol = 2.0 * f64::EPSILON;
    let mut d_lo: f64 = 0.0;
    let mut f_lo = 2.0 * (d_lo.max(1e-300).ln() - b.ln()) + w * (b + d_lo);
    let mut d_hi = b;
    let mut f_hi = 2.0 * (d_hi.ln() - b.ln()) + w * (b + d_hi);
    let mut d = d_lo + (d_hi - d_lo) / 2.0;
    for _ in 0..100 {
        let fd = 2.0 * (d.ln() - b.ln()) + w * (b + d);
        if fd.abs() < tol {
            return d;
        }
        if fd.signum() == f_lo.signum() {
            d_lo = d;
            f_lo = fd;
        } else {
            d_hi = d;
            f_hi = fd;
        }
        let _ = (f_lo, f_hi);
        d = 0.5 * (d_lo + d_hi);
        if (d_hi - d_lo).abs() < tol * d.abs() {
            return d;
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_roundtrip() {
        let t = Transform::Linear { t: 1000.0, a: 0.0 };
        assert!((t.apply(500.0) - 0.5).abs() < 1e-12);
        assert!((t.invert(0.5) - 500.0).abs() < 1e-9);
    }

    #[test]
    fn log_matches_gatingml_example() {
        // Gating-ML 2.0 §6.1.2: flog(10000; T=10000, M=5) = 1, flog(1; T=10000, M=5) = 0.2
        let t = Transform::Log { t: 10000.0, m: 5.0 };
        assert!((t.apply(10000.0) - 1.0).abs() < 1e-12);
        assert!((t.apply(1.0) - 0.2).abs() < 1e-12);
    }

    #[test]
    fn asinh_matches_gatingml_example() {
        // Gating-ML 2.0 §6.1.3: fasinh(-10; T=1000, M=4, A=1) ≈ 0.001 ... use documented values:
        // fasinh(0; 1000, 4, 1) = 0.2, fasinh(1000; 1000,4,1) = 1
        let t = Transform::Asinh { t: 1000.0, m: 4.0, a: 1.0 };
        assert!((t.apply(0.0) - 0.2).abs() < 1e-12);
        assert!((t.apply(1000.0) - 1.0).abs() < 1e-12);
        assert!((t.invert(t.apply(37.5)) - 37.5).abs() < 1e-9);
    }

    #[test]
    fn logicle_matches_gatingml_examples() {
        // Gating-ML 2.0 §6.1.4 worked examples for logicle(T=1000, W=1, M=4, A=0).
        let t = Transform::Logicle { t: 1000.0, w: 1.0, m: 4.0, a: 0.0 };
        let cases = [(-10.0, 0.067574), (-5.0, 0.147986), (-1.0, 0.228752), (0.0, 0.25),
                     (0.3, 0.256384), (1.0, 0.271248), (3.0, 0.312897), (10.0, 0.432426),
                     (100.0, 0.739548), (1000.0, 1.0)];
        for (x, y) in cases {
            let got = t.apply(x);
            assert!((got - y).abs() < 1e-5, "logicle({x}) = {got}, expected {y}");
            assert!((t.invert(got) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn logicle_with_negative_decades() {
        // Gating-ML §6.1.4: logicle(T=1000, W=1, M=4, A=1): f(0)=0.4, f(1000)=1
        let t = Transform::Logicle { t: 1000.0, w: 1.0, m: 4.0, a: 1.0 };
        assert!((t.apply(0.0) - 0.4).abs() < 1e-6);
        assert!((t.apply(1000.0) - 1.0).abs() < 1e-6);
        assert!((t.apply(-10.0) - 0.254059).abs() < 1e-5);
    }

    #[test]
    fn logicle_monotonic_and_ticks() {
        let t = Transform::Logicle { t: 262144.0, w: 0.5, m: 4.5, a: 0.0 };
        let mut prev = f64::NEG_INFINITY;
        for i in -200..=2000 {
            let x = i as f64 * 131.0;
            let y = t.apply(x);
            assert!(y > prev);
            prev = y;
        }
        let ticks = t.ticks();
        assert!(ticks.iter().any(|(_, l, _)| l == "10^5"));
        assert!(ticks.iter().any(|(_, l, _)| l == "0"));
    }
}
