//! Binned plot data for the UI: 2-D density grids and 1-D histograms,
//! computed on transformed (display-space) columns.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Density2D {
    pub bins: usize,
    /// `counts[y * bins + x]`, origin bottom-left, over `x_range` × `y_range` (display space).
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
/// The full (un-zoomed) display range.
pub const FULL_RANGE: [f32; 2] = [DISPLAY_LO, DISPLAY_HI];

/// Validate a requested (zoomed) display range; `None` or an invalid range
/// (non-finite, empty or inverted) falls back to [`FULL_RANGE`].
pub fn view_range(r: Option<[f32; 2]>) -> [f32; 2] {
    match r {
        Some([lo, hi]) if lo.is_finite() && hi.is_finite() && hi - lo > 1e-6 => [lo, hi],
        _ => FULL_RANGE,
    }
}

/// Maps display values onto `bins` cells of `[lo, hi]`. Events beyond a bound
/// that coincides with the edge of the full display range are piled into the
/// edge cell (as in the un-zoomed view); beyond an inner (zoomed) bound they
/// are outside the view and dropped.
#[derive(Clone, Copy)]
struct Axis {
    lo: f32,
    hi: f32,
    scale: f32,
    last: f32,
    pile_lo: bool,
    pile_hi: bool,
}

impl Axis {
    fn new(r: [f32; 2], bins: usize) -> Self {
        let [lo, hi] = view_range(Some(r));
        Axis {
            lo,
            hi,
            scale: bins as f32 / (hi - lo),
            last: (bins - 1) as f32,
            pile_lo: lo <= DISPLAY_LO + 1e-6,
            pile_hi: hi >= DISPLAY_HI - 1e-6,
        }
    }

    #[inline]
    fn bin(&self, v: f32) -> Option<usize> {
        if !v.is_finite() || (v < self.lo && !self.pile_lo) || (v > self.hi && !self.pile_hi) {
            return None;
        }
        Some(((v - self.lo) * self.scale).clamp(0.0, self.last) as usize)
    }

    #[inline]
    fn clamp(&self, v: f32) -> f32 {
        v.clamp(self.lo, self.hi)
    }
}

/// 2-D event counts over the display window `x_range` × `y_range`
/// (use [`FULL_RANGE`] for the un-zoomed view).
pub fn density2d(xs: &[f32], ys: &[f32], mask: Option<&[bool]>, bins: usize, x_range: [f32; 2], y_range: [f32; 2]) -> Density2D {
    let bins = bins.clamp(8, 1024);
    let (ax, ay) = (Axis::new(x_range, bins), Axis::new(y_range, bins));
    let mut counts = vec![0u32; bins * bins];
    let mut total = 0usize;
    for i in 0..xs.len() {
        if let Some(m) = mask {
            if !m[i] {
                continue;
            }
        }
        let (Some(bx), Some(by)) = (ax.bin(xs[i]), ay.bin(ys[i])) else { continue };
        counts[by * bins + bx] += 1;
        total += 1;
    }
    let max_count = counts.iter().copied().max().unwrap_or(0);
    Density2D { bins, counts, max_count, total, x_range: [ax.lo, ax.hi], y_range: [ay.lo, ay.hi] }
}

/// 1-D histogram over the display window `range`.
pub fn hist1d(xs: &[f32], mask: Option<&[bool]>, bins: usize, range: [f32; 2]) -> Hist1D {
    let bins = bins.clamp(8, 4096);
    let ax = Axis::new(range, bins);
    let mut counts = vec![0u32; bins];
    let mut total = 0usize;
    for i in 0..xs.len() {
        if let Some(m) = mask {
            if !m[i] {
                continue;
            }
        }
        let Some(b) = ax.bin(xs[i]) else { continue };
        counts[b] += 1;
        total += 1;
    }
    let max_count = counts.iter().copied().max().unwrap_or(0);
    Hist1D { bins, counts, max_count, total, range: [ax.lo, ax.hi] }
}

/// Pack a subsample of (x, y, density) points inside the density grid's window
/// into a compact byte buffer: `u32 n | f32 x[n] | f32 y[n] | u8 d[n]` where `d`
/// is the density rank (0..=255) of the point's bin, for pseudo-colour dot plots.
pub fn pack_points(xs: &[f32], ys: &[f32], mask: Option<&[bool]>, density: &Density2D, max_points: usize) -> Vec<u8> {
    let bins = density.bins;
    let (ax, ay) = (Axis::new(density.x_range, bins), Axis::new(density.y_range, bins));
    // Sub-sample evenly among the events that fall inside the view.
    let n_total = density.total;
    let stride = if n_total > max_points && max_points > 0 { n_total as f64 / max_points as f64 } else { 1.0 };
    // Log-scaled density rank so sparse regions stay visible.
    let lmax = (density.max_count.max(1) as f32).ln();

    let cap = max_points.min(n_total);
    let mut px: Vec<f32> = Vec::with_capacity(cap);
    let mut py: Vec<f32> = Vec::with_capacity(cap);
    let mut pd: Vec<u8> = Vec::with_capacity(cap);
    let mut next = 0.0f64;
    let mut k = 0usize;
    for i in 0..xs.len() {
        if let Some(m) = mask {
            if !m[i] {
                continue;
            }
        }
        let (x, y) = (xs[i], ys[i]);
        let (Some(bx), Some(by)) = (ax.bin(x), ay.bin(y)) else { continue };
        let take = (k as f64) >= next;
        k += 1;
        if !take {
            continue;
        }
        next += stride;
        let c = density.counts[by * bins + bx].max(1) as f32;
        let d = ((c.ln() / lmax) * 255.0).clamp(0.0, 255.0) as u8;
        px.push(ax.clamp(x));
        py.push(ay.clamp(y));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_range_piles_edges() {
        let xs = [-5.0, 0.0, 0.5, 1.0, 5.0, f32::NAN];
        let h = hist1d(&xs, None, 52, FULL_RANGE);
        assert_eq!(h.total, 5);
        assert_eq!(h.counts[0], 1); // -5 piled into the first cell ...
        assert_eq!(h.counts[51], 2); // ... 1.0 and 5 into the last
        assert_eq!(h.range, FULL_RANGE);
    }

    #[test]
    fn zoomed_range_rebins_and_drops_outside() {
        // 1000 events uniformly in [0.4, 0.5)
        let xs: Vec<f32> = (0..1000).map(|i| 0.4 + 0.1 * i as f32 / 1000.0).collect();
        let full = hist1d(&xs, None, 100, FULL_RANGE);
        let occupied_full = full.counts.iter().filter(|&&c| c > 0).count();
        let zoom = hist1d(&xs, None, 100, [0.4, 0.5]);
        let occupied_zoom = zoom.counts.iter().filter(|&&c| c > 0).count();
        assert!(occupied_full <= 11, "{occupied_full}");
        assert_eq!(occupied_zoom, 100, "zoomed view uses every bin");
        // Events outside an inner bound are dropped, not piled up.
        let narrow = hist1d(&xs, None, 10, [0.45, 0.6]);
        assert_eq!(narrow.total, 500);
        assert!((149..=151).contains(&narrow.counts[0]), "{}", narrow.counts[0]); // bins are 0.015 wide

        let ys = xs.clone();
        let d = density2d(&xs, &ys, None, 64, [0.4, 0.45], [0.4, 0.5]);
        assert_eq!(d.total, 500);
        assert_eq!(d.x_range, [0.4, 0.45]);
        let buf = pack_points(&xs, &ys, None, &d, 100);
        let n = u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize;
        assert_eq!(n, 100);
        for i in 0..n {
            let x = f32::from_le_bytes(buf[4 + i * 4..8 + i * 4].try_into().unwrap());
            assert!((0.4..=0.45).contains(&x), "{x}");
        }
    }

    #[test]
    fn invalid_range_falls_back() {
        assert_eq!(view_range(Some([0.5, 0.5])), FULL_RANGE);
        assert_eq!(view_range(Some([f32::NAN, 1.0])), FULL_RANGE);
        assert_eq!(view_range(None), FULL_RANGE);
        assert_eq!(view_range(Some([0.1, 0.2])), [0.1, 0.2]);
    }
}
