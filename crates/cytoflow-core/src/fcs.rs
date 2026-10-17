//! FCS 2.0 / 3.0 / 3.1 reader (ISAC Flow Cytometry Standard).
//!
//! Supports list-mode data (`$MODE/L`) with integer (`I`), float (`F`) and
//! double (`D`) encodings, both byte orders, per-parameter bit widths,
//! log-amplifier scaling (`$PnE`), gain (`$PnG`) and spillover matrices
//! (`$SPILLOVER`, `SPILL`, `$COMP`, `COMP`).

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// One measured parameter (channel) of an FCS file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Parameter {
    /// `$PnN` short name (e.g. `FL1-H`).
    pub name: String,
    /// `$PnS` stain / long name (e.g. `CD4 FITC`), if present.
    pub stain: Option<String>,
    /// `$PnR` nominal range.
    pub range: f64,
    /// `$PnB` bits.
    pub bits: u32,
    /// `$PnE` amplifier (decades, offset). `(0,0)` means linear.
    pub amplification: (f64, f64),
    /// `$PnG` gain.
    pub gain: f64,
}

impl Parameter {
    /// A display label: `name` plus the stain name when present.
    pub fn label(&self) -> String {
        match &self.stain {
            Some(s) if !s.trim().is_empty() && s.trim() != self.name => {
                format!("{} :: {}", self.name, s.trim())
            }
            _ => self.name.clone(),
        }
    }
}

/// A spillover matrix as read from the TEXT segment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Spillover {
    pub channels: Vec<String>,
    /// Row-major `n x n`; `matrix[i][j]` is the spillover of fluorochrome `i`
    /// into detector `j` (the FCS 3.1 convention).
    pub matrix: Vec<Vec<f64>>,
}

/// A parsed FCS data set (one `$NEXTDATA` segment).
#[derive(Debug, Clone)]
pub struct FcsFile {
    pub version: String,
    pub text: HashMap<String, String>,
    pub parameters: Vec<Parameter>,
    /// Column-major event data: `data[param][event]`, after `$PnE`/`$PnG`
    /// scaling but *before* compensation.
    pub data: Vec<Vec<f32>>,
    pub event_count: usize,
    pub spillover: Option<Spillover>,
}

impl FcsFile {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut f = File::open(path.as_ref())?;
        let mut buf = Vec::new();
        f.seek(SeekFrom::Start(0))?;
        f.read_to_end(&mut buf)?;
        Self::parse(&buf)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 58 {
            return Err(Error::Fcs("file shorter than the 58-byte header".into()));
        }
        let version = String::from_utf8_lossy(&bytes[0..6]).trim().to_string();
        if !version.starts_with("FCS") {
            return Err(Error::Fcs(format!("bad magic {version:?}")));
        }
        let off = |a: usize, b: usize| -> Result<usize> {
            let s = String::from_utf8_lossy(&bytes[a..b]);
            let s = s.trim();
            if s.is_empty() {
                return Ok(0);
            }
            s.parse::<usize>()
                .map_err(|_| Error::Fcs(format!("bad header offset {s:?}")))
        };
        let text_start = off(10, 18)?;
        let text_end = off(18, 26)?;
        let mut data_start = off(26, 34)?;
        let mut data_end = off(34, 42)?;

        if text_end < text_start || text_end >= bytes.len() {
            return Err(Error::Fcs("TEXT segment offsets out of range".into()));
        }
        let mut text = parse_text(&bytes[text_start..=text_end])?;

        // Supplemental TEXT (FCS 3.x).
        let sup_start = kw_usize(&text, "$BEGINSTEXT");
        let sup_end = kw_usize(&text, "$ENDSTEXT");
        if let (Some(s), Some(e)) = (sup_start, sup_end) {
            if s > 0 && e > s && e < bytes.len() && (s, e) != (text_start, text_end) {
                if let Ok(extra) = parse_text(&bytes[s..=e]) {
                    for (k, v) in extra {
                        text.entry(k).or_insert(v);
                    }
                }
            }
        }

        // Header DATA offsets are zero for large files: fall back to TEXT.
        if data_start == 0 || data_end == 0 {
            data_start = kw_usize(&text, "$BEGINDATA").unwrap_or(0);
            data_end = kw_usize(&text, "$ENDDATA").unwrap_or(0);
        }
        if data_start == 0 || data_end <= data_start {
            return Err(Error::Fcs("missing DATA segment offsets".into()));
        }
        if data_end >= bytes.len() {
            // Some writers are off by one on $ENDDATA; clamp rather than fail.
            data_end = bytes.len() - 1;
        }

        let mode = text.get("$MODE").map(|s| s.trim().to_uppercase()).unwrap_or_else(|| "L".into());
        if mode != "L" {
            return Err(Error::Unsupported(format!("$MODE {mode} (only list mode is supported)")));
        }
        let n_par = kw_usize(&text, "$PAR").ok_or_else(|| Error::Fcs("missing $PAR".into()))?;
        let n_tot = kw_usize(&text, "$TOT").ok_or_else(|| Error::Fcs("missing $TOT".into()))?;
        if n_par == 0 {
            return Err(Error::Fcs("$PAR is zero".into()));
        }

        let mut parameters = Vec::with_capacity(n_par);
        for i in 1..=n_par {
            let name = text
                .get(&format!("$P{i}N"))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("P{i}"));
            let stain = text.get(&format!("$P{i}S")).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            let range = text
                .get(&format!("$P{i}R"))
                .and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or(262144.0);
            let bits = text
                .get(&format!("$P{i}B"))
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(32);
            let amplification = text
                .get(&format!("$P{i}E"))
                .and_then(|s| {
                    let mut it = s.split(',');
                    let a = it.next()?.trim().parse::<f64>().ok()?;
                    let b = it.next()?.trim().parse::<f64>().ok()?;
                    Some((a, b))
                })
                .unwrap_or((0.0, 0.0));
            let gain = text
                .get(&format!("$P{i}G"))
                .and_then(|s| s.trim().parse::<f64>().ok())
                .filter(|g| *g > 0.0)
                .unwrap_or(1.0);
            parameters.push(Parameter { name, stain, range, bits, amplification, gain });
        }

        let datatype = text.get("$DATATYPE").map(|s| s.trim().to_uppercase()).unwrap_or_else(|| "F".into());
        let byteord = text.get("$BYTEORD").map(|s| s.trim().to_string()).unwrap_or_else(|| "1,2,3,4".into());
        let big_endian = match byteord.as_str() {
            "4,3,2,1" | "2,1" | "8,7,6,5,4,3,2,1" => true,
            "1,2,3,4" | "1,2" | "1,2,3,4,5,6,7,8" => false,
            other => return Err(Error::Unsupported(format!("$BYTEORD {other}"))),
        };

        let raw = &bytes[data_start..=data_end];
        let data = decode_data(raw, &datatype, big_endian, &parameters, n_tot)?;
        let event_count = data.first().map(|c| c.len()).unwrap_or(0);

        let spillover = parse_spillover(&text);

        Ok(FcsFile { version, text, parameters, data, event_count, spillover })
    }

    pub fn channel_names(&self) -> Vec<String> {
        self.parameters.iter().map(|p| p.name.clone()).collect()
    }

    pub fn channel_index(&self, name: &str) -> Option<usize> {
        self.parameters.iter().position(|p| p.name == name)
    }

    pub fn column(&self, name: &str) -> Result<&[f32]> {
        self.channel_index(name)
            .map(|i| self.data[i].as_slice())
            .ok_or_else(|| Error::ChannelNotFound(name.to_string()))
    }

    /// Keyword lookup (case-insensitive for `$` keywords).
    pub fn keyword(&self, key: &str) -> Option<&str> {
        self.text.get(&key.to_uppercase()).or_else(|| self.text.get(key)).map(|s| s.as_str())
    }
}

fn kw_usize(text: &HashMap<String, String>, key: &str) -> Option<usize> {
    text.get(key).and_then(|s| s.trim().parse::<usize>().ok())
}

/// Parse a TEXT segment: first byte is the delimiter; a doubled delimiter
/// inside a key or value is an escaped literal delimiter.
pub fn parse_text(seg: &[u8]) -> Result<HashMap<String, String>> {
    if seg.is_empty() {
        return Err(Error::Fcs("empty TEXT segment".into()));
    }
    let delim = seg[0];
    let body = &seg[1..];
    let mut tokens: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let b = body[i];
        if b == delim {
            if i + 1 < body.len() && body[i + 1] == delim {
                // Escaped delimiter.
                cur.push(delim);
                i += 2;
                continue;
            }
            tokens.push(std::mem::take(&mut cur));
            i += 1;
        } else {
            cur.push(b);
            i += 1;
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    let mut map = HashMap::new();
    let mut it = tokens.into_iter();
    while let Some(k) = it.next() {
        let v = it.next().unwrap_or_default();
        let key = String::from_utf8_lossy(&k).trim().to_string();
        if key.is_empty() {
            continue;
        }
        let key = if key.starts_with('$') { key.to_uppercase() } else { key };
        let val = String::from_utf8_lossy(&v).to_string();
        map.insert(key, val);
    }
    Ok(map)
}

fn decode_data(
    raw: &[u8],
    datatype: &str,
    big_endian: bool,
    params: &[Parameter],
    n_tot: usize,
) -> Result<Vec<Vec<f32>>> {
    let n_par = params.len();
    let mut cols: Vec<Vec<f32>> = (0..n_par).map(|_| Vec::with_capacity(n_tot)).collect();

    match datatype {
        "F" | "D" => {
            let w = if datatype == "F" { 4 } else { 8 };
            let row_bytes = w * n_par;
            if row_bytes == 0 {
                return Err(Error::Fcs("zero-width row".into()));
            }
            let n_events = (raw.len() / row_bytes).min(if n_tot > 0 { n_tot } else { usize::MAX });
            let mut off = 0;
            for _ in 0..n_events {
                for col in cols.iter_mut() {
                    let chunk = &raw[off..off + w];
                    let v = if w == 4 {
                        let a = [chunk[0], chunk[1], chunk[2], chunk[3]];
                        if big_endian { f32::from_be_bytes(a) } else { f32::from_le_bytes(a) }
                    } else {
                        let a = [chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7]];
                        (if big_endian { f64::from_be_bytes(a) } else { f64::from_le_bytes(a) }) as f32
                    };
                    col.push(v);
                    off += w;
                }
            }
            // Gain applies to floating point data too (FCS 3.1 §3.2.19).
            for (p, col) in cols.iter_mut().enumerate() {
                let g = params[p].gain;
                if (g - 1.0).abs() > f64::EPSILON {
                    let inv = (1.0 / g) as f32;
                    for v in col.iter_mut() {
                        *v *= inv;
                    }
                }
            }
        }
        "I" => {
            let widths: Vec<usize> = params.iter().map(|p| (p.bits as usize).div_ceil(8)).collect();
            if widths.iter().any(|w| *w == 0 || *w > 8) || params.iter().any(|p| p.bits % 8 != 0) {
                return Err(Error::Unsupported(
                    "integer data with bit widths that are not a multiple of 8".into(),
                ));
            }
            let row_bytes: usize = widths.iter().sum();
            let n_events = (raw.len() / row_bytes).min(if n_tot > 0 { n_tot } else { usize::MAX });
            // Masks: values are masked to the next power of two >= $PnR (FCS 3.1 §3.2.18).
            let masks: Vec<u64> = params
                .iter()
                .map(|p| {
                    let r = p.range.max(1.0) as u64;
                    let np2 = r.next_power_of_two();
                    if np2 >= 1 << 63 { u64::MAX } else { np2 - 1 }
                })
                .collect();
            let scalers: Vec<Box<dyn Fn(u64) -> f32>> = params
                .iter()
                .map(|p| {
                    let (f1, f2) = p.amplification;
                    let r = p.range;
                    let g = p.gain;
                    if f1 > 0.0 {
                        let f2 = if f2 <= 0.0 { 1.0 } else { f2 };
                        Box::new(move |v: u64| (10f64.powf(f1 * v as f64 / r) * f2) as f32) as Box<dyn Fn(u64) -> f32>
                    } else if (g - 1.0).abs() > f64::EPSILON {
                        Box::new(move |v: u64| (v as f64 / g) as f32) as Box<dyn Fn(u64) -> f32>
                    } else {
                        Box::new(|v: u64| v as f32) as Box<dyn Fn(u64) -> f32>
                    }
                })
                .collect();
            let mut off = 0;
            for _ in 0..n_events {
                for p in 0..n_par {
                    let w = widths[p];
                    let chunk = &raw[off..off + w];
                    let mut v: u64 = 0;
                    if big_endian {
                        for b in chunk {
                            v = (v << 8) | *b as u64;
                        }
                    } else {
                        for b in chunk.iter().rev() {
                            v = (v << 8) | *b as u64;
                        }
                    }
                    v &= masks[p];
                    cols[p].push(scalers[p](v));
                    off += w;
                }
            }
        }
        "A" => return Err(Error::Unsupported("ASCII ($DATATYPE/A) data".into())),
        other => return Err(Error::Unsupported(format!("$DATATYPE {other}"))),
    }
    Ok(cols)
}

/// Parse `$SPILLOVER` (FCS 3.1) or the legacy `SPILL`, `$COMP`, `COMP` keywords.
pub fn parse_spillover(text: &HashMap<String, String>) -> Option<Spillover> {
    let raw = ["$SPILLOVER", "SPILL", "SPILLOVER", "$COMP", "COMP"]
        .iter()
        .find_map(|k| text.get(*k))?;
    parse_spillover_str(raw)
}

pub fn parse_spillover_str(raw: &str) -> Option<Spillover> {
    let parts: Vec<&str> = raw.split(',').map(|s| s.trim()).collect();
    let n: usize = parts.first()?.parse().ok()?;
    if n == 0 || parts.len() < 1 + n + n * n {
        return None;
    }
    let channels: Vec<String> = parts[1..1 + n].iter().map(|s| s.to_string()).collect();
    let vals: Vec<f64> = parts[1 + n..1 + n + n * n]
        .iter()
        .map(|s| s.parse::<f64>().ok())
        .collect::<Option<Vec<_>>>()?;
    let matrix = (0..n).map(|i| vals[i * n..(i + 1) * n].to_vec()).collect();
    Some(Spillover { channels, matrix })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_escaped_delimiters() {
        let seg = b"/$P1N/FSC//A/$TOT/10/";
        let m = parse_text(seg).unwrap();
        assert_eq!(m["$P1N"], "FSC/A");
        assert_eq!(m["$TOT"], "10");
    }

    #[test]
    fn spillover_parse() {
        let s = parse_spillover_str("2,A,B,1,0.1,0.2,1").unwrap();
        assert_eq!(s.channels, vec!["A", "B"]);
        assert_eq!(s.matrix, vec![vec![1.0, 0.1], vec![0.2, 1.0]]);
    }
}
