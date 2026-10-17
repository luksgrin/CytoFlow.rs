//! Tauri commands. Every command runs its work on a blocking worker thread
//! (never on the main thread) and maps all errors to `String`.

use crate::state::{AppState, Inner};
use cytoflow_core::compensation::CompMatrix;
use cytoflow_core::plot;
use cytoflow_core::stats::ChannelStats;
use cytoflow_core::workspace::{CompSource, PopulationStats};
use cytoflow_core::{export, wsp, GateDim, GateShape, Transform, TransformKind, Workspace};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager};

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Run `f` with the locked state on a blocking worker thread.
async fn with_state<T, F>(app: AppHandle, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Inner) -> CmdResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut guard = state.lock();
        f(&mut guard)
    })
    .await
    .map_err(err)?
}

/// Run `f` on a blocking worker thread without touching the state.
async fn blocking<T, F>(f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> CmdResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

#[derive(Serialize)]
pub struct Tick {
    pos: f64,
    label: String,
    major: bool,
}

fn ticks_for(ws: &Workspace, channel: &str) -> Vec<Tick> {
    ws.transform_for(channel)
        .ticks()
        .into_iter()
        .map(|(pos, label, major)| Tick { pos, label, major })
        .collect()
}

/// Axis label for a channel of a sample, e.g. `FL1-H :: CD4 FITC`.
fn axis_label(ws: &Workspace, sample_id: &str, dim: &GateDim) -> String {
    let base = ws
        .samples
        .iter()
        .find(|s| s.id == sample_id)
        .and_then(|s| s.channels.iter().find(|p| p.name == dim.channel))
        .map(|p| p.label())
        .unwrap_or_else(|| dim.channel.clone());
    let compensated = dim.compensated
        && ws.comp_matrix_for(sample_id).is_some_and(|m| m.channels.iter().any(|c| c == &dim.channel));
    if compensated {
        format!("Comp-{base}")
    } else {
        base
    }
}

fn mask_for(ws: &mut Workspace, sample_id: &str, gate_id: Option<&str>) -> CmdResult<Option<Arc<Vec<bool>>>> {
    match gate_id {
        Some(g) => Ok(Some(ws.membership(sample_id, g).map_err(err)?)),
        None => {
            // Validate the sample id even for the root population.
            ws.fcs(sample_id).map_err(err)?;
            Ok(None)
        }
    }
}

fn errors_to_strings(ws: &Workspace, errors: Vec<(String, cytoflow_core::Error)>) -> Vec<String> {
    errors
        .into_iter()
        .map(|(id, e)| {
            let name = ws.samples.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or(id);
            format!("{name}: {e}")
        })
        .collect()
}

// ------------------------------------------------------------- workspace

#[tauri::command]
pub async fn workspace_new(app: AppHandle) -> CmdResult<Value> {
    with_state(app, |st| {
        *st = Inner::default();
        st.view()
    })
    .await
}

#[tauri::command]
pub async fn workspace_get(app: AppHandle) -> CmdResult<Value> {
    with_state(app, |st| st.view()).await
}

#[tauri::command]
pub async fn workspace_open(app: AppHandle, path: String) -> CmdResult<Value> {
    // Parse and load FCS files without holding the lock.
    let p = PathBuf::from(&path);
    let (ws, errors) = blocking(move || Workspace::load(&p).map_err(err)).await?;
    with_state(app, move |st| {
        let errors = errors_to_strings(&ws, errors);
        st.workspace = ws;
        st.path = Some(PathBuf::from(path));
        Ok(json!({ "workspace": st.view()?, "errors": errors }))
    })
    .await
}

#[tauri::command]
pub async fn workspace_save(app: AppHandle, path: String) -> CmdResult<Value> {
    with_state(app, move |st| {
        st.workspace.save(&path).map_err(err)?;
        st.path = Some(PathBuf::from(path));
        st.view()
    })
    .await
}

#[tauri::command]
pub async fn workspace_import_wsp(app: AppHandle, path: String, search_dirs: Vec<String>) -> CmdResult<Value> {
    let dirs: Vec<PathBuf> = search_dirs.into_iter().map(PathBuf::from).collect();
    let import = blocking(move || wsp::import_wsp(&path, &dirs).map_err(err)).await?;
    with_state(app, move |st| {
        let missing: Vec<Value> = import
            .missing_files
            .iter()
            .map(|(name, p)| json!({ "name": name, "path": p.to_string_lossy() }))
            .collect();
        st.workspace = import.workspace;
        st.path = None;
        Ok(json!({ "workspace": st.view()?, "warnings": import.warnings, "missing": missing }))
    })
    .await
}

// --------------------------------------------------------------- samples

#[tauri::command]
pub async fn sample_add(app: AppHandle, paths: Vec<String>) -> CmdResult<Value> {
    with_state(app, move |st| {
        let mut errors = Vec::new();
        for p in &paths {
            if let Err(e) = st.workspace.add_sample(p) {
                let name = std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.clone());
                errors.push(format!("{name}: {e}"));
            }
        }
        Ok(json!({ "workspace": st.view()?, "errors": errors }))
    })
    .await
}

#[tauri::command]
pub async fn sample_remove(app: AppHandle, sample_id: String) -> CmdResult<Value> {
    with_state(app, move |st| {
        st.workspace.remove_sample(&sample_id).map_err(err)?;
        st.view()
    })
    .await
}

#[tauri::command]
pub async fn sample_keywords(app: AppHandle, sample_id: String) -> CmdResult<BTreeMap<String, String>> {
    with_state(app, move |st| match st.workspace.fcs(&sample_id) {
        Ok(fcs) => Ok(fcs.text.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        // Not loaded: fall back to the keywords stored in the workspace.
        Err(e) => st
            .workspace
            .samples
            .iter()
            .find(|s| s.id == sample_id)
            .map(|s| s.keywords.clone())
            .ok_or_else(|| e.to_string()),
    })
    .await
}

// ------------------------------------------------- transforms & compensation

#[tauri::command]
pub async fn channel_set_transform(app: AppHandle, channel: String, transform: Transform) -> CmdResult<Value> {
    with_state(app, move |st| {
        st.workspace.set_transform(&channel, transform);
        st.view()
    })
    .await
}

#[tauri::command]
pub async fn channel_default_transform(
    app: AppHandle,
    channel: String,
    kind: String,
    sample_id: String,
) -> CmdResult<Transform> {
    // Parsed through serde so that new core transform kinds need no backend change.
    let kind: TransformKind = serde_json::from_value(serde_json::json!({ "kind": kind.to_ascii_lowercase() }))
        .map_err(|_| format!("Unknown transform kind: {kind}"))?;
    with_state(app, move |st| {
        let fcs = st.workspace.fcs(&sample_id).map_err(err)?;
        let param = fcs
            .parameters
            .iter()
            .find(|p| p.name == channel)
            .ok_or_else(|| format!("Channel not found: {channel}"))?;
        let column = st.workspace.column(&sample_id, &channel, false).map_err(err)?;
        Ok(Transform::default_for(kind, param.range, Some(&column)))
    })
    .await
}

#[tauri::command]
pub async fn compensation_set(app: AppHandle, comp: CompSource) -> CmdResult<Value> {
    with_state(app, move |st| {
        st.workspace.set_compensation(comp);
        st.view()
    })
    .await
}

#[tauri::command]
pub async fn compensation_matrix(app: AppHandle, sample_id: String) -> CmdResult<Option<CompMatrix>> {
    with_state(app, move |st| Ok(st.workspace.comp_matrix_for(&sample_id))).await
}

// ----------------------------------------------------------------- gates

#[tauri::command]
pub async fn gate_add(
    app: AppHandle,
    name: String,
    parent: Option<String>,
    shape: GateShape,
    negate: bool,
) -> CmdResult<Value> {
    with_state(app, move |st| {
        let id = st.workspace.add_gate(&name, parent.as_deref(), shape, negate).map_err(err)?;
        Ok(json!({ "id": id, "workspace": st.view()? }))
    })
    .await
}

#[tauri::command]
pub async fn gate_update(
    app: AppHandle,
    gate_id: String,
    shape: Option<GateShape>,
    name: Option<String>,
    negate: Option<bool>,
    color: Option<Option<String>>,
) -> CmdResult<Value> {
    with_state(app, move |st| {
        st.workspace.update_gate(&gate_id, shape, name.as_deref(), negate, color).map_err(err)?;
        st.view()
    })
    .await
}

#[tauri::command]
pub async fn gate_remove(app: AppHandle, gate_id: String) -> CmdResult<Value> {
    with_state(app, move |st| {
        st.workspace.remove_gate(&gate_id).map_err(err)?;
        st.view()
    })
    .await
}

// ------------------------------------------------------------ statistics

#[tauri::command]
pub async fn population_stats(app: AppHandle, sample_id: String) -> CmdResult<Vec<PopulationStats>> {
    with_state(app, move |st| st.workspace.all_population_stats(&sample_id).map_err(err)).await
}

#[tauri::command]
pub async fn channel_stats(
    app: AppHandle,
    sample_id: String,
    gate_id: Option<String>,
    channel: String,
    compensated: bool,
) -> CmdResult<ChannelStats> {
    with_state(app, move |st| {
        st.workspace
            .channel_statistics(&sample_id, gate_id.as_deref(), &channel, compensated)
            .map_err(err)
    })
    .await
}

// ----------------------------------------------------------------- plots

#[tauri::command]
pub async fn axis_ticks(app: AppHandle, channel: String) -> CmdResult<Vec<Tick>> {
    with_state(app, move |st| Ok(ticks_for(&st.workspace, &channel))).await
}

#[tauri::command]
pub async fn plot_hist(
    app: AppHandle,
    sample_id: String,
    gate_id: Option<String>,
    x: GateDim,
    bins: usize,
) -> CmdResult<Value> {
    // Gather cached columns under the lock, bin outside it.
    let (xs, mask, ticks, label) = with_state(app, move |st| {
        let ws = &mut st.workspace;
        let mask = mask_for(ws, &sample_id, gate_id.as_deref())?;
        let xs = ws.transformed(&sample_id, &x.channel, x.compensated).map_err(err)?;
        Ok((xs, mask, ticks_for(ws, &x.channel), axis_label(ws, &sample_id, &x)))
    })
    .await?;
    blocking(move || {
        let h = plot::hist1d(&xs, mask.as_deref().map(|m| m.as_slice()), bins);
        Ok(json!({
            "bins": h.bins,
            "counts": h.counts,
            "max_count": h.max_count,
            "total": h.total,
            "range": h.range,
            "ticks": ticks,
            "label": label,
        }))
    })
    .await
}

/// `"CFP1"` little-endian.
const PLOT_MAGIC: u32 = 0x3150_4643;

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn plot_2d(
    app: AppHandle,
    sample_id: String,
    gate_id: Option<String>,
    x: GateDim,
    y: GateDim,
    bins: usize,
    max_points: usize,
) -> CmdResult<Response> {
    let (xs, ys, mask, header) = with_state(app, move |st| {
        let ws = &mut st.workspace;
        let mask = mask_for(ws, &sample_id, gate_id.as_deref())?;
        let xs = ws.transformed(&sample_id, &x.channel, x.compensated).map_err(err)?;
        let ys = ws.transformed(&sample_id, &y.channel, y.compensated).map_err(err)?;
        let header = json!({
            "x_label": axis_label(ws, &sample_id, &x),
            "y_label": axis_label(ws, &sample_id, &y),
            "x_ticks": ticks_for(ws, &x.channel),
            "y_ticks": ticks_for(ws, &y.channel),
        });
        Ok((xs, ys, mask, header))
    })
    .await?;
    blocking(move || {
        let mask = mask.as_deref().map(|m| m.as_slice());
        let d = plot::density2d(&xs, &ys, mask, bins);
        let points = plot::pack_points(&xs, &ys, mask, &d, max_points);
        let mut header = header;
        if let Value::Object(m) = &mut header {
            m.insert("total".into(), json!(d.total));
            m.insert("max_count".into(), json!(d.max_count));
            m.insert("bins".into(), json!(d.bins));
        }
        let header = serde_json::to_vec(&header).map_err(err)?;
        let mut out = Vec::with_capacity(12 + header.len() + d.counts.len() * 4 + points.len());
        out.extend_from_slice(&PLOT_MAGIC.to_le_bytes());
        out.extend_from_slice(&(header.len() as u32).to_le_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(&(d.bins as u32).to_le_bytes());
        for c in &d.counts {
            out.extend_from_slice(&c.to_le_bytes());
        }
        out.extend_from_slice(&points);
        Ok(Response::new(out))
    })
    .await
}

// ---------------------------------------------------------------- export

#[tauri::command]
pub async fn export_population_csv(
    app: AppHandle,
    sample_id: String,
    gate_id: Option<String>,
    path: String,
    channels: Option<Vec<String>>,
    compensated: bool,
    transformed: bool,
) -> CmdResult<usize> {
    let (headers, columns, mask) = with_state(app, move |st| {
        let ws = &mut st.workspace;
        let mask = mask_for(ws, &sample_id, gate_id.as_deref())?;
        let channels = match channels {
            Some(c) => c,
            None => ws.fcs(&sample_id).map_err(err)?.channel_names(),
        };
        let mut columns = Vec::with_capacity(channels.len());
        for c in &channels {
            let col = if transformed {
                ws.transformed(&sample_id, c, compensated)
            } else {
                ws.column(&sample_id, c, compensated)
            }
            .map_err(err)?;
            columns.push(col);
        }
        Ok((channels, columns, mask))
    })
    .await?;
    blocking(move || {
        let filtered: Vec<Vec<f32>> = columns
            .iter()
            .map(|col| match &mask {
                Some(m) => col.iter().zip(m.iter()).filter(|(_, &keep)| keep).map(|(&v, _)| v).collect(),
                None => col.to_vec(),
            })
            .collect();
        let rows = filtered.first().map_or(0, |c| c.len());
        let file = File::create(&path).map_err(err)?;
        export::write_csv(BufWriter::new(file), &headers, &filtered).map_err(err)?;
        Ok(rows)
    })
    .await
}

#[tauri::command]
pub async fn export_stats_csv(app: AppHandle, path: String) -> CmdResult<usize> {
    let rows = with_state(app, |st| {
        let ws = &mut st.workspace;
        let order = ws.preorder();
        let samples: Vec<(String, String)> = ws
            .samples
            .iter()
            .filter(|s| ws.is_loaded(&s.id))
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect();
        let mut rows = Vec::new();
        for (id, name) in samples {
            let pops = std::iter::once(None).chain(order.iter().map(|g| Some(g.as_str())));
            for g in pops {
                let s = ws.population_stats(&id, g).map_err(err)?;
                let pop_path = match g {
                    Some(g) => ws.gate_path(g),
                    None => s.name.clone(),
                };
                rows.push(vec![
                    name.clone(),
                    pop_path,
                    s.count.to_string(),
                    format!("{:.4}", s.pct_parent),
                    format!("{:.4}", s.pct_total),
                ]);
            }
        }
        Ok(rows)
    })
    .await?;
    blocking(move || {
        let headers: Vec<String> =
            ["sample", "population", "count", "pct_parent", "pct_total"].iter().map(|s| s.to_string()).collect();
        let file = File::create(&path).map_err(err)?;
        export::write_table(BufWriter::new(file), &headers, &rows).map_err(err)?;
        Ok(rows.len())
    })
    .await
}

// ------------------------------------------------------------------ files

/// Write a PNG/JPEG/SVG (or any binary) produced by the frontend, given as base64.
#[tauri::command]
pub async fn export_image(path: String, base64_data: String) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = base64_decode(&base64_data).ok_or("invalid base64 image data")?;
        std::fs::write(&path, bytes).map_err(|e| format!("{path}: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Write a UTF-8 text file (SVG, CSV, JSON) produced by the frontend.
#[tauri::command]
pub async fn write_text_file(path: String, contents: String) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || std::fs::write(&path, contents).map_err(|e| format!("{path}: {e}")))
        .await
        .map_err(|e| e.to_string())?
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.split_once(',').map(|(_, b)| b).unwrap_or(s); // strip a data: URL prefix
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a') as u32 + 26,
            b'0'..=b'9' => (c - b'0') as u32 + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let bytes: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace() && *b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut acc: u32 = 0;
        for (i, b) in chunk.iter().enumerate() {
            acc |= val(*b)? << (18 - 6 * i);
        }
        let n = chunk.len();
        if n >= 2 { out.push((acc >> 16) as u8); }
        if n >= 3 { out.push((acc >> 8) as u8); }
        if n == 4 { out.push(acc as u8); }
    }
    Some(out)
}
