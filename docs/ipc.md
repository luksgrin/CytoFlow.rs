# CytoFlow.rs — frontend ⇄ backend IPC contract

Tauri 2 commands exposed by `src-tauri` (Rust) and called from `src/` (TypeScript)
via `invoke(name, args)`. Argument names are given in **camelCase** exactly as the
frontend must send them (Tauri converts them to the Rust snake_case parameters).
All JSON shapes are the `serde` serialisations of the `cytoflow-core` types, so the
backend must *not* re-define them; it passes core types straight through.

The backend holds exactly one `cytoflow_core::Workspace` in managed state
(`Mutex<Workspace>`). Every mutating command returns the fresh `WorkspaceView`.

## Shared JSON types (from `cytoflow-core`)

```ts
type Transform =
  | { kind: "linear";  t: number; a: number }
  | { kind: "log";     t: number; m: number }
  | { kind: "asinh";   t: number; m: number; a: number }
  | { kind: "logicle"; t: number; w: number; m: number; a: number }
  | { kind: "biex";    t: number; w: number; pos: number; neg: number }; // FlowJo-style: w = width basis (≤ 0, e.g. -100)
// All transforms map the instrument range onto ~[0, 1] ("display space").
// Plots cover display range [-0.02, 1.02] on each axis (events are clamped into it).

type GateDim = { channel: string; compensated: boolean };

type GateShape =
  | { type: "rectangle"; dims: GateDim[]; min: (number|null)[]; max: (number|null)[] } // 1 or 2 dims, null = open bound
  | { type: "polygon";   dims: [GateDim, GateDim]; vertices: [number, number][] }
  | { type: "ellipse";   dims: [GateDim, GateDim]; center: [number, number]; a: number; b: number; angle: number } // radians
  | { type: "boolean";   op: "and" | "or" | "not"; refs: string[] };                   // refs = gate ids
// ALL gate coordinates are in display space (after transform), never raw values.

type Gate = { shape: GateShape; negate: boolean; grid?: number | null }; // grid: optional display-grid rounding (set by the .wsp importer for FlowJo parity)
type GateNode = { id: string; name: string; parent: string | null; gate: Gate; color: string | null };

type Parameter = { name: string; stain: string | null; range: number; bits: number; amplification: [number, number]; gain: number };
type SampleInfo = { id: string; name: string; path: string; event_count: number; channels: Parameter[]; keywords: Record<string, string> };

type CompMatrix = { channels: string[]; spillover: number[][] };  // row-major spillover
type CompSource = { source: "none" } | { source: "fcs" } | { source: "custom"; matrix: CompMatrix };

type WorkspaceView = {
  version: number; name: string;
  samples: SampleInfo[];
  channel_settings: Record<string, { transform: Transform }>;   // keyed by channel name
  compensation: CompSource;
  gates: GateNode[];                                            // flat list, tree via `parent`
  loaded: string[];                                             // sample ids whose FCS data is in memory
  path: string | null;                                          // where the workspace was last saved/opened
};

type PopulationStats = { gate_id: string | null; name: string; count: number; parent_count: number; total: number; pct_parent: number; pct_total: number };
type ChannelStats    = { channel: string; mean: number; median: number; sd: number; cv: number; geo_mean: number | null; min: number; max: number };
type Tick            = { pos: number; label: string; major: boolean };  // pos in display space
```

## Commands

| command | args | returns |
|---|---|---|
| `workspace_new` | – | `WorkspaceView` |
| `workspace_get` | – | `WorkspaceView` |
| `workspace_open` | `{ path }` | `{ workspace: WorkspaceView, errors: string[] }` (our JSON format) |
| `workspace_save` | `{ path }` | `WorkspaceView` |
| `workspace_import_wsp` | `{ path, searchDirs: string[] }` | `{ workspace: WorkspaceView, warnings: string[], missing: {name: string, path: string}[] }` |
| `sample_add` | `{ paths: string[] }` | `{ workspace: WorkspaceView, errors: string[] }` |
| `sample_remove` | `{ sampleId }` | `WorkspaceView` |
| `sample_keywords` | `{ sampleId }` | `Record<string,string>` (full TEXT segment) |
| `channel_set_transform` | `{ channel, transform: Transform }` | `WorkspaceView` (gate coords are re-mapped by the core) |
| `channel_default_transform` | `{ channel, kind: "linear"\|"log"\|"asinh"\|"logicle"\|"biex", sampleId }` | `Transform` (defaults estimated from that sample's data) |
| `compensation_set` | `{ comp: CompSource }` | `WorkspaceView` |
| `compensation_matrix` | `{ sampleId }` | `CompMatrix \| null` (effective matrix for that sample) |
| `gate_add` | `{ name, parent: string\|null, shape: GateShape, negate: boolean }` | `{ id: string, workspace: WorkspaceView }` |
| `gate_update` | `{ gateId, shape?: GateShape, name?: string, negate?: boolean, color?: string\|null }` | `WorkspaceView` (omit `color` to keep it, `null` clears it) |
| `gate_remove` | `{ gateId }` | `WorkspaceView` (descendants removed too) |
| `population_stats` | `{ sampleId }` | `PopulationStats[]` — root ("All events", gate_id null) first, then gates in pre-order |
| `channel_stats` | `{ sampleId, gateId: string\|null, channel, compensated }` | `ChannelStats` |
| `axis_ticks` | `{ channel }` | `Tick[]` |
| `plot_hist` | `{ sampleId, gateId: string\|null, x: GateDim, bins }` | `{ bins, counts: number[], max_count, total, range: [lo, hi], ticks: Tick[], label: string }` |
| `plot_2d` | `{ sampleId, gateId: string\|null, x: GateDim, y: GateDim, bins, maxPoints }` | **binary** (see below) |
| `export_population_csv` | `{ sampleId, gateId: string\|null, path, channels: string[]\|null, compensated, transformed }` | `number` (rows written) |
| `export_image` | `{ path, base64Data }` | `null` — writes binary data (PNG/JPEG; a `data:` URL prefix is stripped) |
| `write_text_file` | `{ path, contents }` | `null` — writes a UTF-8 text file (SVG, JSON) |
| `export_stats_csv` | `{ path }` | `number` (rows) — one row per sample × population: sample, population path, count, %parent, %total |

Errors: every command returns `Err(String)` with a human-readable message; the frontend shows it in the status bar.

### `plot_2d` binary layout (little-endian)

Returned as `tauri::ipc::Response` (raw bytes); on the frontend `invoke` yields an `ArrayBuffer`.

```
u32  magic = 0x31504643  ("CFP1")
u32  header_len
u8[header_len]  header JSON: { x_label, y_label, x_ticks: Tick[], y_ticks: Tick[], total, max_count, bins }
u32  bins
u32[bins*bins]  counts   (index = y*bins + x; origin bottom-left; both axes cover display [-0.02, 1.02])
u32  n
f32[n] x        (display space, already clamped into range)
f32[n] y
u8[n]  d        (0..255 log-density rank of the point's bin, for pseudo-colour)
```

`maxPoints` caps `n` by even sub-sampling; the density grid always uses all events.

## File dialogs

The frontend uses `@tauri-apps/plugin-dialog` (`open`/`save`) and passes the chosen
paths to the commands above. The backend only needs `dialog:default` and the
`core:default` permissions; file reading is done in Rust, not via the fs plugin.

## Workspace file format

`*.cfw.json` — the `serde_json` of `Workspace` (samples, channel_settings,
compensation, gates). Sample paths are stored absolute; on open the core also
looks for the file name next to the workspace file.
