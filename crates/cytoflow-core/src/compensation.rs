//! Fluorescence compensation: `compensated = raw · S⁻¹` where `S` is the
//! spillover matrix (rows = fluorochromes, columns = detectors).

use crate::error::{Error, Result};
use crate::fcs::{FcsFile, Spillover};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CompMatrix {
    pub channels: Vec<String>,
    /// Row-major spillover matrix.
    pub spillover: Vec<Vec<f64>>,
}

impl From<Spillover> for CompMatrix {
    fn from(s: Spillover) -> Self {
        CompMatrix { channels: s.channels, spillover: s.matrix }
    }
}

impl CompMatrix {
    pub fn identity(channels: Vec<String>) -> Self {
        let n = channels.len();
        let spillover = (0..n).map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect()).collect();
        CompMatrix { channels, spillover }
    }

    pub fn is_identity(&self) -> bool {
        self.spillover.iter().enumerate().all(|(i, r)| {
            r.iter().enumerate().all(|(j, v)| (v - if i == j { 1.0 } else { 0.0 }).abs() < 1e-12)
        })
    }

    pub fn inverse(&self) -> Result<Vec<Vec<f64>>> {
        invert(&self.spillover).ok_or_else(|| Error::Compensation("spillover matrix is singular".into()))
    }

    /// Apply compensation to an FCS file, returning a new column set where the
    /// matrix channels have been compensated and the rest are untouched.
    pub fn apply(&self, fcs: &FcsFile) -> Result<Vec<Vec<f32>>> {
        let inv = self.inverse()?;
        let n = self.channels.len();
        let idx: Vec<usize> = self
            .channels
            .iter()
            .map(|c| fcs.channel_index(c).ok_or_else(|| Error::ChannelNotFound(c.clone())))
            .collect::<Result<_>>()?;
        let mut out = fcs.data.clone();
        let n_events = fcs.event_count;
        let mut row = vec![0f64; n];
        #[allow(clippy::needless_range_loop)]
        for e in 0..n_events {
            for (k, &ci) in idx.iter().enumerate() {
                row[k] = fcs.data[ci][e] as f64;
            }
            for (j, &cj) in idx.iter().enumerate() {
                let acc: f64 = row.iter().zip(inv.iter()).map(|(r, inv_row)| r * inv_row[j]).sum();
                out[cj][e] = acc as f32;
            }
        }
        Ok(out)
    }
}

/// Gauss-Jordan inverse with partial pivoting.
pub fn invert(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = m.len();
    if n == 0 || m.iter().any(|r| r.len() != n) {
        return None;
    }
    let mut a: Vec<Vec<f64>> = m.iter().map(|r| {
        let mut row = r.clone();
        row.resize(2 * n, 0.0);
        row
    }).collect();
    for i in 0..n {
        a[i][n + i] = 1.0;
    }
    for col in 0..n {
        let pivot = (col..n).max_by(|&x, &y| a[x][col].abs().partial_cmp(&a[y][col].abs()).unwrap())?;
        if a[pivot][col].abs() < 1e-14 {
            return None;
        }
        a.swap(col, pivot);
        let p = a[col][col];
        for v in a[col].iter_mut() {
            *v /= p;
        }
        for r in 0..n {
            if r != col {
                let factor = a[r][col];
                if factor != 0.0 {
                    let pivot_row = a[col].clone();
                    for (dst, src) in a[r].iter_mut().zip(pivot_row.iter()) {
                        *dst -= factor * src;
                    }
                }
            }
        }
    }
    Some(a.into_iter().map(|r| r[n..].to_vec()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invert_2x2() {
        let m = vec![vec![1.0, 0.1], vec![0.2, 1.0]];
        let inv = invert(&m).unwrap();
        let det = 1.0 - 0.02;
        assert!((inv[0][0] - 1.0 / det).abs() < 1e-12);
        assert!((inv[0][1] + 0.1 / det).abs() < 1e-12);
    }
}
