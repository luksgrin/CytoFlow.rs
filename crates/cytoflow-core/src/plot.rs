//! Binned plot data for the UI: 2-D density grids and 1-D histograms,
//! computed on transformed (display-space) columns.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Density2D {
    pub bins: usize,
    /// `counts[y * bins + x]`, origin bottom-left, display space `[lo, hi]` on both axes.
    pub counts: Vec<u32>,
    pub max_count: u32,
    pub total: usize,
    pub x_range: [f32; 2],
    pub y_range: [f32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hist1D {
    pub bins: usize,
    pub counts: Vec<u32>,
    pub max_count: u32,
    pub total: usize,
    pub range: [f32; 2],
}

/// Display range used for every transformed axis. Slightly wider than `[0,1]`
/// so that events clamped at the edges are still visible.
pub const DISPLAY_LO: f32 = -0.02;
pub const DISPLAY_HI: f32 = 1.02;

pub fn density2d(xs: &[f32], ys: &[f32], mask: Option<&[bool]>, bins: usize) -> Density2D {
    let bins = bins.clamp(8, 1024);
    let mut counts = vec![0u32; bins * bins];
    let span = DISPLAY_HI - DISPLAY_LO;
    let scale = bins as f32 / span;
    let mut total = 0usize;
    let last = (bins - 1) as f32;
    for i in 0..xs.len() {
        if let Some(m) = mask {
            if !m[i] {
                continue;
            }
        }
        let (x, y) = (xs[i], ys[i]);
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        let bx = ((x - DISPLAY_LO) * scale).clamp(0.0, last) as usize;
        let by = ((y - DISPLAY_LO) * scale).clamp(0.0, last) as usize;
        counts[by * bins + bx] += 1;
        total += 1;
    }
    let max_count = counts.iter().copied().max().unwrap_or(0);
    Density2D { bins, counts, max_count, total, x_range: [DISPLAY_LO, DISPLAY_HI], y_range: [DISPLAY_LO, DISPLAY_HI] }
}

pub fn hist1d(xs: &[f32], mask: Option<&[bool]>, bins: usize) -> Hist1D {
    let bins = bins.clamp(8, 4096);
    let mut counts = vec![0u32; bins];
    let span = DISPLAY_HI - DISPLAY_LO;
    let scale = bins as f32 / span;
    let last = (bins - 1) as f32;
    let mut total = 0usize;
    for i in 0..xs.len() {
        if let Some(m) = mask {
            if !m[i] {
                continue;
            }
        }
        let x = xs[i];
        if !x.is_finite() {
            continue;
        }
        let b = ((x - DISPLAY_LO) * scale).clamp(0.0, last) as usize;
        counts[b] += 1;
        total += 1;
    }
    let max_count = counts.iter().copied().max().unwrap_or(0);
    Hist1D { bins, counts, max_count, total, range: [DISPLAY_LO, DISPLAY_HI] }
}

/// Pack a subsample of (x, y, density) points into a compact byte buffer:
/// `u32 n | f32 x[n] | f32 y[n] | u8 d[n]` where `d` is the density rank
/// (0..=255) of the point's bin, for pseudo-colour dot plots.
pub fn pack_points(xs: &[f32], ys: &[f32], mask: Option<&[bool]>, density: &Density2D, max_points: usize) -> Vec<u8> {
    let n_total = match mask {
        Some(m) => m.iter().filter(|&&b| b).count(),
        None => xs.len(),
    };
    let stride = if n_total > max_points && max_points > 0 { n_total as f64 / max_points as f64 } else { 1.0 };
    let bins = density.bins;
    let span = DISPLAY_HI - DISPLAY_LO;
    let scale = bins as f32 / span;
    let last = (bins - 1) as f32;
    // Log-scaled density rank so sparse regions stay visible.
    let lmax = (density.max_count.max(1) as f32).ln();

    let mut px: Vec<f32> = Vec::with_capacity(max_points.min(n_total));
    let mut py: Vec<f32> = Vec::with_capacity(max_points.min(n_total));
    let mut pd: Vec<u8> = Vec::with_capacity(max_points.min(n_total));
    let mut next = 0.0f64;
    let mut k = 0usize;
    for i in 0..xs.len() {
        if let Some(m) = mask {
            if !m[i] {
                continue;
            }
        }
        let take = (k as f64) >= next;
        k += 1;
        if !take {
            continue;
        }
        next += stride;
        let (x, y) = (xs[i], ys[i]);
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        let bx = ((x - DISPLAY_LO) * scale).clamp(0.0, last) as usize;
        let by = ((y - DISPLAY_LO) * scale).clamp(0.0, last) as usize;
        let c = density.counts[by * bins + bx].max(1) as f32;
        let d = ((c.ln() / lmax) * 255.0).clamp(0.0, 255.0) as u8;
        px.push(x.clamp(DISPLAY_LO, DISPLAY_HI));
        py.push(y.clamp(DISPLAY_LO, DISPLAY_HI));
        pd.push(d);
    }
    let n = px.len() as u32;
    let mut out = Vec::with_capacity(4 + px.len() * 9);
    out.extend_from_slice(&n.to_le_bytes());
    for v in &px {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in &py {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&pd);
    out
}
