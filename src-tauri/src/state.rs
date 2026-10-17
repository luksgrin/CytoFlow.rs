//! Managed application state: the single open workspace and its file path.

use cytoflow_core::Workspace;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

#[derive(Default)]
pub struct Inner {
    pub workspace: Workspace,
    /// Where the workspace was last saved to / opened from.
    pub path: Option<PathBuf>,
}

impl Inner {
    /// `serde_json` of the workspace (`groups`, `sample_gates`,
    /// `sample_channel_settings`, `layouts`, `ui_state`, ...) plus `gates` (the "All Samples"
    /// template, as in version 1), `loaded` and `path`: the `WorkspaceView`.
    pub fn view(&self) -> Result<Value, String> {
        let mut v = serde_json::to_value(&self.workspace).map_err(|e| e.to_string())?;
        let loaded: Vec<String> = self
            .workspace
            .samples
            .iter()
            .filter(|s| self.workspace.is_loaded(&s.id))
            .map(|s| s.id.clone())
            .collect();
        if let Value::Object(map) = &mut v {
            map.insert("gates".into(), serde_json::to_value(self.workspace.gates()).map_err(|e| e.to_string())?);
            map.insert("loaded".into(), serde_json::to_value(loaded).map_err(|e| e.to_string())?);
            map.insert(
                "path".into(),
                self.path.as_ref().map(|p| Value::String(p.to_string_lossy().into_owned())).unwrap_or(Value::Null),
            );
        }
        Ok(v)
    }
}

#[derive(Default)]
pub struct AppState(Mutex<Inner>);

impl AppState {
    /// Lock the state, recovering from a poisoned mutex (a panic in an earlier
    /// command must not brick the app).
    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}
