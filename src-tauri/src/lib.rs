//! CytoFlow desktop shell: a thin Tauri 2 layer over `cytoflow-core`.
//!
//! The IPC contract implemented here is documented in `docs/ipc.md`.

mod commands;
mod state;

use tauri::Manager;

pub use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            // Files passed on the command line (`open CytoFlow.app --args a.fcs b.wsp`)
            // are loaded before the window asks for the workspace.
            let paths: Vec<std::path::PathBuf> = std::env::args_os().skip(1).map(std::path::PathBuf::from).filter(|p| p.is_file()).collect();
            if !paths.is_empty() {
                let state: tauri::State<AppState> = app.state();
                let mut st = state.lock();
                for (name, err) in open_paths(&mut st, &paths) {
                    eprintln!("CytoFlow: could not open {name}: {err}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::workspace_new,
            commands::workspace_get,
            commands::workspace_open,
            commands::workspace_save,
            commands::workspace_import_wsp,
            commands::sample_add,
            commands::sample_remove,
            commands::sample_keywords,
            commands::channel_set_transform,
            commands::channel_default_transform,
            commands::compensation_set,
            commands::compensation_matrix,
            commands::gate_add,
            commands::gate_update,
            commands::gate_remove,
            commands::gate_copy_to_group,
            commands::group_list,
            commands::group_create,
            commands::group_set_members,
            commands::group_remove,
            commands::population_stats,
            commands::channel_stats,
            commands::axis_ticks,
            commands::plot_hist,
            commands::plot_2d,
            commands::export_population_csv,
            commands::export_stats_csv,
            commands::export_image,
            commands::write_text_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running CytoFlow");
}

/// Open a mix of FCS files, CytoFlow workspaces (`*.json`) and FlowJo `.wsp`
/// files into the state. Returns `(file name, error)` for failures.
fn open_paths(st: &mut state::Inner, paths: &[std::path::PathBuf]) -> Vec<(String, String)> {
    let mut errors = Vec::new();
    for p in paths {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let result: Result<(), String> = match ext.as_str() {
            "json" => cytoflow_core::Workspace::load(p).map(|(ws, errs)| {
                st.workspace = ws;
                st.path = Some(p.clone());
                errors.extend(errs.into_iter().map(|(id, e)| (id, e.to_string())));
            }).map_err(|e| e.to_string()),
            "wsp" => cytoflow_core::wsp::import_wsp(p, &[]).map(|imp| {
                st.workspace = imp.workspace;
                st.path = None;
                errors.extend(imp.missing_files.into_iter().map(|(n, path)| (n, format!("missing file {}", path.display()))));
            }).map_err(|e| e.to_string()),
            _ => st.workspace.add_sample(p).map(|_| ()).map_err(|e| e.to_string()),
        };
        if let Err(e) = result {
            errors.push((name, e));
        }
    }
    errors
}
