# CytoFlow desktop shell (`src-tauri`)

Tauri 2 backend for CytoFlow.rs. It holds a single `cytoflow_core::Workspace`
in managed state and exposes it to the frontend through the IPC contract in
[`../docs/ipc.md`](../docs/ipc.md). All analysis lives in `crates/cytoflow-core`;
this crate only wires commands, state and the window.

## Commands

| command | purpose |
|---|---|
| `workspace_new`, `workspace_get` | reset / fetch the `WorkspaceView` |
| `workspace_open`, `workspace_save` | load / save `*.cfw.json` |
| `workspace_import_wsp` | import a FlowJo `.wsp` |
| `sample_add`, `sample_remove`, `sample_keywords` | manage FCS samples |
| `channel_set_transform`, `channel_default_transform`, `axis_ticks` | axis transforms |
| `compensation_set`, `compensation_matrix` | compensation |
| `gate_add`, `gate_update`, `gate_remove` | gate hierarchy |
| `population_stats`, `channel_stats` | statistics |
| `plot_hist`, `plot_2d` (binary `CFP1` response) | plot data |
| `export_population_csv`, `export_stats_csv` | CSV export |

Every command runs on a blocking worker thread (`spawn_blocking`), never on the
main thread; errors are returned as plain strings.

## Running

From the repository root (needs the frontend's `npm install` first):

```sh
cargo tauri dev     # runs `npm run dev` (Vite on :1420) and opens the app
cargo tauri build   # runs `npm run build`, bundles CytoFlow.app + .dmg
cargo build -p cytoflow   # backend only, no frontend needed
```

Bundles land in `target/release/bundle/{macos,dmg}`.
