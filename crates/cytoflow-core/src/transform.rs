//! Axis transforms, following the Gating-ML 2.0 definitions so that every
//! transform maps the instrument range onto roughly `[0, 1]`.
//!
//! * `flin(x; T, A) = (x + A) / (T + A)`
//! * `flog(x; T, M) = log10(x / T) / M + 1`
//! * `fasinh(x; T, M, A) = (asinh(x·sinh(M·ln10)/T) + A·ln10) / ((M + A)·ln10)`
//! * `logicle(x; T, W, M, A)` — Parks, Roederer & Moore, Cytometry A 69A:541 (2006)
//! * `biex(x; T, widthBasis, pos, neg)` — the FlowJo "biexponential" scale, see [`Biex`].

use serde::{Deserialize, Serialize};

const LN10: f64 = std::f64::consts::LN_10;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TransformKind {
    Linear,
    Log,
    Asinh,
    Logicle,
    Biex,
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
    /// FlowJo-style biexponential: `T` top of scale (`maxRange`), `W` the
    /// width basis (`≤ 0`, e.g. `-10`, `-100`), `pos` positive decades and
    /// `neg` extra negative decades. See [`Biex`] for the exact definition.
    Biex { t: f64, w: f64, pos: f64, neg: f64 },
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
            Transform::Biex { .. } => TransformKind::Biex,
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
            TransformKind::Biex => Transform::Biex { t, w: -100.0, pos: 4.5, neg: 0.0 },
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
            Transform::Biex { t, w, pos, neg } => Biex::new(t, w, pos, neg).scale(x),
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
            Transform::Biex { t, w, pos, neg } => Biex::new(t, w, pos, neg).inverse(y),
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
            Transform::Biex { t, w, pos, neg } => {
                let bx = Biex::new(t, w, pos, neg);
                out.extend(xs.iter().map(|&x| bx.scale(x as f64) as f32));
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
                    Transform::Log { t, .. }
                    | Transform::Asinh { t, .. }
                    | Transform::Logicle { t, .. }
                    | Transform::Biex { t, .. } => t,
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

impl Transform {
    /// Ticks for a (possibly zoomed) display window `[lo, hi]`: the regular
    /// ticks inside the window, or — when fewer than two labelled ticks would
    /// be visible — "nice" round values in data space so a zoomed axis stays readable.
    pub fn ticks_in(&self, lo: f64, hi: f64) -> Vec<(f64, String, bool)> {
        let base: Vec<_> = self.ticks().into_iter().filter(|t| t.0 >= lo - 1e-9 && t.0 <= hi + 1e-9).collect();
        if base.iter().filter(|t| t.2 && !t.1.is_empty()).count() >= 2 {
            return base;
        }
        let (v0, v1) = (self.invert(lo), self.invert(hi));
        if !v0.is_finite() || !v1.is_finite() || v1 <= v0 {
            return base;
        }
        let step = nice_step((v1 - v0) / 5.0);
        let mut out = Vec::new();
        let mut k = (v0 / step).ceil();
        while k * step <= v1 + step * 1e-9 && out.len() < 50 {
            let v = if (k * step).abs() < step * 1e-9 { 0.0 } else { k * step };
            let y = self.apply(v);
            if y.is_finite() && y >= lo - 1e-9 && y <= hi + 1e-9 {
                out.push((y, format_value(v, step), true));
            }
            k += 1.0;
        }
        if out.len() < 2 {
            return base;
        }
        out
    }
}

/// Round value label for a tick spacing `step` (`25K`, `1.5K`, `250`, `0.05`).
fn format_value(v: f64, step: f64) -> String {
    let dec = |s: f64| if s >= 1.0 { 0 } else { (-s.log10()).ceil() as usize };
    let s = if step >= 1000.0 || v.abs() >= 10_000.0 {
        format!("{:.*}K", dec(step / 1000.0), v / 1000.0)
    } else {
        format!("{:.*}", dec(step), v)
    };
    if s.starts_with("-0") && s.trim_start_matches(['-', '0', '.', 'K']).is_empty() { s[1..].to_string() } else { s }
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

/// Number of channels of the FlowJo biexponential lookup table. FlowJo, and
/// the open-source re-implementations of its biex scale (cytolib's
/// `biexpTrans`, FlowKit's `generate_biex_lut`), build the scale as a table
/// over `0..=4096` channels; the display value is `channel / 4096`.
pub const BIEX_CHANNELS: f64 = 4096.0;

/// The FlowJo "biexponential" (`biex`) scale.
///
/// FlowJo parameterises it by the top of scale `T` (`maxRange`), a *width
/// basis* `W ≤ 0` (`width`, typically −10 … −1000), the positive decades
/// `pos` and the extra negative decades `neg`. Like the logicle scale, the
/// display value `y` is defined through the inverse function
/// `x = a·e^{b·u} − c·e^{−d·u} − f` (for `u ≥ u₀`, mirrored as
/// `x(u) = −x(2u₀ − u)` below the zero point `u₀`).
///
/// The relations below were derived from the published description of the
/// biex scale (FlowJo documentation of width basis / positive / negative
/// decades; the CytoML/flowWorkspace documentation of `flowjo_biexp`, whose
/// arguments are `channelRange`, `maxValue`, `pos`, `neg`, `widthBasis`) and
/// from reading the open-source table generator in FlowKit
/// (`flowkit/_models/transforms/_wsp_transforms.py::generate_biex_lut`,
/// BSD-3-Clause, itself documented as a port of cytolib's `biexpTrans`).
/// This is an independent analytic implementation of the same function
/// (no lookup table, no interpolation). With `N = 4096` channels:
///
/// * `width = log10(−W)` (the width basis is a linear-region size in raw
///   units; `|W|` is clamped to `≥ 1`),
/// * `D₀ = pos − width/2`, `E = max(neg, 0) + width/2`,
/// * zero channel `z = ⌊N·E / (E + D₀)⌋` (at most `N/2`), and the total
///   decades are re-derived from it: `D = N·E / z` (`D = D₀` if `z = 0`),
/// * `w = width / (2D)`, `b = D·ln 10`, and `d` solves
///   `2·(ln d − ln b) + w·(b + d) = 0` (the same equation as logicle's, Parks
///   et al. 2006 / Gating-ML 2.0 §6.1.4 appendix),
/// * `a = T·e^{−b}`, `c = a·e^{(b + d)·(w + E/D)}`,
/// * the table index runs over `u = channel / (N + 1)`, so `u₀ = z / (N + 1)`
///   and `f = a·e^{b·u₀} − c·e^{−d·u₀}` (so that `x(u₀) = 0`),
/// * the display value is `y = channel / N = u·(N + 1)/N`.
///
/// Relation to logicle: up to the channel quantisation (`z` rounded to an
/// integer channel, `u` normalised by `N + 1`) and the normalisation
/// `a = T·e^{−b}` (logicle instead solves for `a` so that `x(1) = T` exactly),
/// this is the logicle scale with `M = pos`, `A = neg` and
/// `W_logicle = log10(−W) / 2` — the same correspondence that CytoML uses
/// when it maps FlowJo `logicle` transforms onto `biexpTrans`
/// (`widthBasis = −10^{2·W}`). For `T = 262144, W = −100, pos = 4.42` the
/// two scales agree to ~10⁻³ of the display range (see the unit tests); the
/// earlier approximation `W_logicle = (M − log10(T/|W|))/2` is markedly
/// different.
#[derive(Debug, Clone, Copy)]
pub struct Biex {
    t: f64,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    f: f64,
    /// Zero point in table units `u`.
    u0: f64,
    /// `(N + 1) / N`: table units → display units.
    k: f64,
}

impl Biex {
    pub fn new(t: f64, width_basis: f64, pos: f64, neg: f64) -> Self {
        Self::with_channels(t, width_basis, pos, neg, BIEX_CHANNELS)
    }

    /// The biex scale built over `n` table channels instead of
    /// [`BIEX_CHANNELS`] (the channel count enters through the zero channel
    /// `z = ⌊n·E / (E + D₀)⌋` and the derived total decades `D = n·E / z`).
    pub fn with_channels(t: f64, width_basis: f64, pos: f64, neg: f64, channels: f64) -> Self {
        let n = if channels >= 2.0 && channels.is_finite() { channels.floor() } else { BIEX_CHANNELS };
        let t = if t > 0.0 && t.is_finite() { t } else { 262144.0 };
        let pos = if pos > 0.0 && pos.is_finite() { pos } else { 4.5 };
        let neg = if neg.is_finite() { neg.max(0.0) } else { 0.0 };
        let wb = if width_basis.is_finite() { width_basis.abs().max(1.0) } else { 10.0 };
        let width = wb.log10();
        let d0 = pos - width / 2.0;
        let extra = neg + width / 2.0;
        // Zero channel and total decades.
        let z = if extra + d0 > 0.0 { (n * extra / (extra + d0)).floor().min((n / 2.0).floor()).max(0.0) } else { 0.0 };
        let decades = if z > 0.0 { n * extra / z } else { d0.max(1e-3) };
        let w = width / (2.0 * decades);
        let b = decades * LN10;
        let d = solve_d(b, w);
        let a = t * (-b).exp();
        let c = a * ((b + d) * (w + extra / decades)).exp();
        let u0 = z / (n + 1.0);
        let f = a * (b * u0).exp() - c * (-d * u0).exp();
        Biex { t, a, b, c, d, f, u0, k: (n + 1.0) / n }
    }

    /// Raw value at table position `u`.
    fn raw_at(&self, u: f64) -> f64 {
        let negative = u < self.u0;
        let u = if negative { 2.0 * self.u0 - u } else { u };
        let v = self.a * (self.b * u).exp() - self.c * (-self.d * u).exp() - self.f;
        if negative { -v } else { v }
    }

    /// `dx/du`.
    fn slope_at(&self, u: f64) -> f64 {
        let u = if u < self.u0 { 2.0 * self.u0 - u } else { u };
        self.a * self.b * (self.b * u).exp() + self.c * self.d * (-self.d * u).exp()
    }

    /// Display scale → raw.
    pub fn inverse(&self, y: f64) -> f64 {
        self.raw_at(y / self.k)
    }

    /// Display value of raw zero (`z / N`).
    pub fn zero(&self) -> f64 {
        self.u0 * self.k
    }

    /// Raw → display scale (safeguarded Newton iteration on `u`).
    pub fn scale(&self, x: f64) -> f64 {
        if !x.is_finite() {
            return if x.is_nan() { f64::NAN } else if x > 0.0 { 2.0 } else { -1.0 };
        }
        let (mut lo, mut hi) = (self.u0 - 1.0, 1.0);
        while self.raw_at(lo) > x && lo > -50.0 {
            lo -= 1.0;
        }
        while self.raw_at(hi) < x && hi < 50.0 {
            hi += 1.0;
        }
        let mut u = 0.5 * (lo + hi);
        for _ in 0..80 {
            let fu = self.raw_at(u) - x;
            if fu.abs() < 1e-12 * self.t {
                break;
            }
            if fu > 0.0 { hi = u } else { lo = u }
            let mut next = u - fu / self.slope_at(u);
            if !(next > lo && next < hi) || !next.is_finite() {
                next = 0.5 * (lo + hi);
            }
            if (next - u).abs() < 1e-14 {
                u = next;
                break;
            }
            u = next;
        }
        u * self.k
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
    fn zoomed_ticks_stay_readable() {
        let lin = Transform::Linear { t: 262144.0, a: 0.0 };
        let tk = lin.ticks_in(0.30, 0.35);
        assert!(tk.len() >= 2, "{tk:?}");
        assert!(tk.iter().all(|t| t.2 && (0.30..=0.35).contains(&t.0) && t.1.ends_with('K')), "{tk:?}");
        let log = Transform::Log { t: 262144.0, m: 4.5 };
        // Full window: the regular decade ticks.
        assert_eq!(log.ticks_in(-0.02, 1.02), log.ticks());
        // Inside one decade: round values (e.g. 2K, 4K, ...) instead of a lone label.
        let (lo, hi) = (log.apply(1500.0), log.apply(9000.0));
        let tk = log.ticks_in(lo, hi);
        assert!(tk.iter().filter(|t| !t.1.is_empty()).count() >= 2, "{tk:?}");
        assert_eq!(format_value(2500.0, 500.0), "2500");
        assert_eq!(format_value(12500.0, 500.0), "12.5K");
        assert_eq!(format_value(250.0, 50.0), "250");
        assert_eq!(format_value(0.05, 0.01), "0.05");
        assert_eq!(format_value(-0.0, 1.0), "0");
    }

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

    const POS: f64 = 4.418539922;

    #[test]
    fn biex_monotonic() {
        for (wb, neg) in [(-10.0, 0.0), (-100.0, 0.0), (-1000.0, 1.0), (-1.0, 0.0)] {
            let t = Transform::Biex { t: 262144.0, w: wb, pos: POS, neg };
            let mut prev = f64::NEG_INFINITY;
            for i in -2000..=2000 {
                let x = (i as f64 / 2000.0).powi(3) * 262144.0;
                let y = t.apply(x);
                assert!(y > prev, "biex(wb={wb}) not increasing at {x}");
                prev = y;
            }
        }
    }

    #[test]
    fn biex_roundtrip() {
        let t = Transform::Biex { t: 262144.0, w: -100.0, pos: POS, neg: 0.5 };
        for x in [-5000.0, -100.0, -1.0, 0.0, 0.5, 3.0, 47.0, 1234.5, 1e5, 262144.0] {
            let y = t.apply(x);
            let back = t.invert(y);
            assert!((back - x).abs() <= 1e-7 * x.abs().max(1.0), "{x} -> {y} -> {back}");
        }
        for i in 0..=100 {
            let y = i as f64 / 100.0;
            assert!((t.apply(t.invert(y)) - y).abs() < 1e-10);
        }
    }

    #[test]
    fn biex_zero_and_top() {
        // W = -100 → width = 2 decades, E = 1, D₀ = pos − 1: the zero channel
        // is ⌊4096 / pos⌋ = 927.
        let t = Transform::Biex { t: 262144.0, w: -100.0, pos: POS, neg: 0.0 };
        assert!((t.apply(0.0) - 927.0 / 4096.0).abs() < 1e-12);
        assert!(t.invert(927.0 / 4096.0).abs() < 1e-9);
        // The top of scale lands on the last channel up to the table's
        // (N + 1) normalisation: within 10⁻³ of 1.
        let top = t.apply(262144.0);
        assert!((top - 1.0).abs() < 1e-3, "biex(T) = {top}");
        // W = -10 → width = 1: zero at ⌊4096·0.5 / (pos)⌋ / 4096.
        let t10 = Transform::Biex { t: 262144.0, w: -10.0, pos: POS, neg: 0.0 };
        assert!((t10.apply(0.0) - (4096.0f64 * 0.5 / POS).floor() / 4096.0).abs() < 1e-12);
        // Extra negative decades move the zero up.
        let tn = Transform::Biex { t: 262144.0, w: -10.0, pos: POS, neg: 1.0 };
        assert!(tn.apply(0.0) > t10.apply(0.0) + 0.15);
    }

    #[test]
    fn biex_vs_logicle() {
        // Matching logicle (W = log10|W_biex| / 2, M = pos, A = neg): the same
        // family of curves, equal to within the table quantisation.
        // The former importer approximation W = (M − log10(T/|W_biex|)) / 2
        // (= 0.5 here) is a clearly different scale.
        let (t, wb) = (262144.0, -100.0);
        let bx = Transform::Biex { t, w: wb, pos: POS, neg: 0.0 };
        let matched = Transform::Logicle { t, w: 1.0, m: POS, a: 0.0 };
        let approx_w = (POS - (t / 100.0f64).log10()) / 2.0;
        let approx = Transform::Logicle { t, w: approx_w, m: POS, a: 0.0 };
        let (mut d_matched, mut d_approx) = (0.0f64, 0.0f64);
        for i in -100..=1000 {
            let x = if i < 0 { i as f64 * 20.0 } else { (i as f64 / 1000.0).powi(4) * t };
            d_matched = d_matched.max((bx.apply(x) - matched.apply(x)).abs());
            d_approx = d_approx.max((bx.apply(x) - approx.apply(x)).abs());
        }
        println!("max |biex − logicle(W=1)| = {d_matched:.2e}, max |biex − logicle(W={approx_w})| = {d_approx:.4}");
        assert!(d_matched < 1e-3, "biex vs matching logicle: {d_matched}");
        assert!(d_approx > 0.03, "biex vs old approximation: {d_approx}");
        assert!(bx.apply(-2000.0) < 0.0);
    }

    #[test]
    fn biex_defaults_and_ticks() {
        let t = Transform::default_for(TransformKind::Biex, 262144.0, None);
        assert_eq!(t, Transform::Biex { t: 262144.0, w: -100.0, pos: 4.5, neg: 0.0 });
        assert_eq!(t.kind(), TransformKind::Biex);
        let ticks = t.ticks();
        assert!(ticks.iter().any(|(_, l, _)| l == "10^4"));
        assert!(ticks.iter().any(|(_, l, _)| l == "0"));
        assert!(ticks.windows(2).all(|w| w[0].0 <= w[1].0));
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#"{"kind":"biex","t":262144.0,"w":-100.0,"pos":4.5,"neg":0.0}"#);
        let back: Transform = serde_json::from_str(r#"{"kind":"biex","t":1000,"w":-10,"pos":4,"neg":1}"#).unwrap();
        assert_eq!(back, Transform::Biex { t: 1000.0, w: -10.0, pos: 4.0, neg: 1.0 });
        let mut out = Vec::new();
        t.apply_slice(&[0.0, 100.0], &mut out);
        assert!((out[0] as f64 - t.apply(0.0)).abs() < 1e-6);
    }
}
