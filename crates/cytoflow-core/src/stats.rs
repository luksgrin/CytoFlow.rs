//! Population statistics.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ChannelStats {
    pub channel: String,
    pub mean: f64,
    pub median: f64,
    pub sd: f64,
    pub cv: f64,
    pub geo_mean: Option<f64>,
    pub min: f64,
    pub max: f64,
}

pub fn channel_stats(channel: &str, values: &[f32], mask: Option<&[bool]>) -> ChannelStats {
    let mut v: Vec<f64> = match mask {
        Some(m) => values.iter().zip(m).filter(|(_, &k)| k).map(|(x, _)| *x as f64).collect(),
        None => values.iter().map(|x| *x as f64).collect(),
    };
    let n = v.len();
    if n == 0 {
        return ChannelStats { channel: channel.to_string(), ..Default::default() };
    }
    let mean = v.iter().sum::<f64>() / n as f64;
    let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n.max(2) - 1) as f64;
    let sd = var.sqrt();
    let cv = if mean.abs() > 0.0 { 100.0 * sd / mean } else { 0.0 };
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 };
    let geo_mean = if v[0] > 0.0 { Some((v.iter().map(|x| x.ln()).sum::<f64>() / n as f64).exp()) } else { None };
    ChannelStats { channel: channel.to_string(), mean, median, sd, cv, geo_mean, min: v[0], max: v[n - 1] }
}

/// Percentile (0..100) of the masked values.
pub fn percentile(values: &[f32], mask: Option<&[bool]>, p: f64) -> f64 {
    let mut v: Vec<f32> = match mask {
        Some(m) => values.iter().zip(m).filter(|(_, &k)| k).map(|(x, _)| *x).collect(),
        None => values.to_vec(),
    };
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((p / 100.0) * (v.len() - 1) as f64).round() as usize;
    v[idx.min(v.len() - 1)] as f64
}
