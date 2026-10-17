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
type SampleGateNode = GateNode & { tailored: boolean };          // node of a sample's own tree (see "Groups")

type Group = { id: string; name: string; sample_ids: string[]; gates: GateNode[] };  // gates = template tree
type GateScope = { group: string } | { sample: string };          // where a gate edit applies

type Parameter = { name: string; stain: string | null; range: number; bits: number; amplification: [number, number]; gain: number };
type SampleInfo = { id: string; name: string; path: string; event_count: number; channels: Parameter[]; keywords: Record<string, string> };

type CompMatrix = { channels: string[]; spillover: number[][] };  // row-major spillover
type CompSource = { source: "none" } | { source: "fcs" } | { source: "custom"; matrix: CompMatrix };

type Layout = { id: string; name: string; data: unknown };        // data = the frontend's layout document, stored verbatim

type WorkspaceView = {
  version: number; name: string;                                // version = 2
  samples: SampleInfo[];
  channel_settings: Record<string, { transform: Transform }>;   // default transform, keyed by channel name
  sample_channel_settings: Record<string, Record<string, { transform: Transform }>>; // sample id → channel → override
  compensation: CompSource;
  groups: Group[];                                              // groups[0] is always { id: "all", name: "All Samples" }
  sample_gates: Record<string, SampleGateNode[]>;               // sample id → that sample's own tree (flat, tree via `parent`)
  gates: GateNode[];                                            // = groups[0].gates (the "All Samples" template), kept for compatibility
  layouts: Layout[];                                            // named layout-editor pages (saved in the workspace file)
  ui_state: unknown;                                            // UI preferences (last axes per sample, overlays); null if never set
  loaded: string[];                                             // sample ids whose FCS data is in memory
  path: string | null;                                          // where the workspace was last saved/opened
};

type PopulationStats = { gate_id: string | null; name: string; count: number; parent_count: number; total: number; pct_parent: number; pct_total: number };
type ChannelStats    = { channel: string; mean: number; median: number; sd: number; cv: number; geo_mean: number | null; min: number; max: number };
type Tick            = { pos: number; label: string; major: boolean };  // pos in display space
```

## Groups and per-sample gate trees

* Every sample owns its effective gate tree (`sample_gates[sampleId]`); all
  per-sample commands (`population_stats`, `channel_stats`, `plot_*`,
  `export_population`, `export_population_csv`) resolve `gateId` in **that sample's** tree.
* A group's `gates` are a *template*. The same population has the same gate id
  in the template and in every member's copy, so a gate id selected in one
  sample can be plotted in another sample of the group.
* `gate_add` with `{ group }` adds the gate to the template and copies it into
  every member that has the parent population; with `{ sample }` it is added to
  that sample only (marked `tailored`).
* `gate_update` with `{ group }` updates the template; every member's copy gets
  the new `name` / `color`, and the new `shape` / `negate` unless the copy is
  `tailored`. With `{ sample }` only that sample's copy changes; a change of
  shape, negation or name marks it `tailored: true`.
* `gate_remove` with `{ group }` removes the gate and its descendants from the
  template and from every member sample; with `{ sample }` from that sample only.
* `gate_copy_to_group` ("Apply to group") pushes a sample's gate to a group: the
  template takes it (inserted if new; its parent must be in the template), every
  member's copy is reset to it (or inserted) and is no longer tailored.
* Omitted `scope` = `{ group: "all" }` (the version-1 behaviour).
* Transforms: `channel_settings` is the default; `sample_channel_settings`
  holds per-sample overrides. Template coordinates are in the default space, a
  sample's tree in that sample's effective space; copying between them re-maps
  overridden channels. `channel_set_transform` without `sampleId` changes the
  default (re-mapping the templates and the trees of samples without an
  override on that channel); with `sampleId` it sets that sample's override and
  re-maps only its tree (setting the default transform removes the override).
* "All Samples" always contains every sample (new samples get a copy of its
  template) and cannot be edited by `group_set_members` or removed. Adding a
  sample to a group copies the template gates it lacks; removing it keeps them.

## Commands

| command | args | returns |
|---|---|---|
| `workspace_new` | – | `WorkspaceView` |
| `workspace_get` | – | `WorkspaceView` |
| `workspace_open` | `{ path }` | `{ workspace: WorkspaceView, errors: string[] }` (our JSON format) |
| `workspace_save` | `{ path }` | `WorkspaceView` |
| `workspace_import_wsp` | `{ path, searchDirs: string[] }` | `{ workspace: WorkspaceView, warnings: string[], missing: {name: string, path: string}[] }` |
| `layouts_get` | – | `Layout[]` |
| `layouts_set` | `{ layouts: Layout[] }` | `WorkspaceView` (replaces all layouts; saved by `workspace_save`) |
| `ui_state_get` | – | `unknown` (`null` when never set) |
| `ui_state_set` | `{ uiState: unknown }` | `WorkspaceView` (replaces the UI state; saved by `workspace_save`) |
| `sample_add` | `{ paths: string[] }` | `{ workspace: WorkspaceView, errors: string[] }` |
| `sample_remove` | `{ sampleId }` | `WorkspaceView` |
| `sample_keywords` | `{ sampleId }` | `Record<string,string>` (full TEXT segment) |
| `channel_set_transform` | `{ channel, transform: Transform, sampleId?: string }` | `WorkspaceView` (default transform, or that sample's override; gate coords are re-mapped by the core) |
| `channel_default_transform` | `{ channel, kind: "linear"\|"log"\|"asinh"\|"logicle"\|"biex", sampleId }` | `Transform` (defaults estimated from that sample's data) |
| `compensation_set` | `{ comp: CompSource }` | `WorkspaceView` |
| `compensation_matrix` | `{ sampleId }` | `CompMatrix \| null` (effective matrix for that sample) |
| `group_list` | – | `Group[]` |
| `group_create` | `{ name, sampleIds: string[] }` | `{ id: string, workspace: WorkspaceView }` |
| `group_set_members` | `{ groupId, sampleIds: string[] }` | `WorkspaceView` (error for `"all"`) |
| `group_remove` | `{ groupId }` | `WorkspaceView` (error for `"all"`; members keep their gates) |
| `gate_add` | `{ name, parent: string\|null, shape: GateShape, negate: boolean, scope?: GateScope }` | `{ id: string, workspace: WorkspaceView }` (`parent` must exist in the scope's tree) |
| `gate_update` | `{ gateId, shape?: GateShape, name?: string, negate?: boolean, color?: string\|null, scope?: GateScope }` | `WorkspaceView` (omit `color` to keep it, `null` clears it) |
| `gate_remove` | `{ gateId, scope?: GateScope }` | `WorkspaceView` (descendants removed too) |
| `gate_copy_to_group` | `{ gateId, fromSampleId, groupId }` | `WorkspaceView` |
| `population_stats` | `{ sampleId }` | `PopulationStats[]` — root ("All events", gate_id null) first, then gates in pre-order |
| `channel_stats` | `{ sampleId, gateId: string\|null, channel, compensated }` | `ChannelStats` |
| `axis_ticks` | `{ channel, sampleId?: string }` | `Tick[]` (that sample's effective transform, else the default) |
| `plot_hist` | `{ sampleId, gateId: string\|null, x: GateDim, bins, range?: [lo, hi] }` | `{ bins, counts: number[], max_count, total, range: [lo, hi], ticks: Tick[], label: string }` — see *Zoomed views* |
| `plot_2d` | `{ sampleId, gateId: string\|null, x: GateDim, y: GateDim, bins, maxPoints, xRange?: [lo, hi], yRange?: [lo, hi] }` | **binary** (see below) — see *Zoomed views* |
| `export_population` | `{ sampleId, gateId: string\|null, path, format?: "csv"\|"xlsx"\|"parquet", channels: string[]\|null, compensated, transformed }` | `number` (rows written) — see *Population export* |
| `export_population_csv` | `{ sampleId, gateId: string\|null, path, channels: string[]\|null, compensated, transformed }` | `number` (rows written) — alias of `export_population` with `format: "csv"` |
| `export_image` | `{ path, base64Data }` | `null` — writes binary data (PNG/JPEG; a `data:` URL prefix is stripped) |
| `write_text_file` | `{ path, contents }` | `null` — writes a UTF-8 text file (SVG, JSON) |
| `export_stats_csv` | `{ path }` | `number` (rows) — one row per sample × population: sample, population path, count, %parent, %total |

Errors: every command returns `Err(String)` with a human-readable message; the frontend shows it in the status bar.

### Updates

Implemented in `src-tauri/src/updates.rs` on top of `tauri-plugin-updater`
(endpoint and public key in `tauri.conf.json` → `plugins.updater`). The version
policy is enforced here, not in the UI, and the webview is not granted the
plugin's own `updater:*` commands, so it cannot install anything else.

| command | args | returns |
|---|---|---|
| `update_check` | `{ includePrerelease: boolean, skipVersion?: string }` | `UpdateInfo` |
| `update_install` | `{ version: string, includePrerelease?: boolean }` | does not return on success (the app relaunches); `Err` otherwise |

```ts
interface UpdateInfo {
  available: boolean;       // can be installed in place now
  version: string | null;   // newest published version, when newer than the running one
  currentVersion: string;
  notes: string | null;     // release notes from latest.json
  date: string | null;      // RFC 3339 publication date
  skipped: boolean;         // version == skipVersion (then available is false)
  reason: "major" | "prerelease" | "deb" | null; // why a newer version is not available
  url: string | null;       // releases page for that version, set when reason is set
}
```

* `update_check` reads `latest.json` from the latest GitHub release. A version
  whose **major** number differs from the running one is reported with
  `available: false, reason: "major"`; pre-releases with `reason: "prerelease"`
  unless `includePrerelease` (the release workflow never publishes a manifest for
  a pre-release, so this cannot happen today); a Debian/RPM package install with
  `reason: "deb"` (the package manager owns those files). `skipVersion` is the
  version the user chose to skip (kept by the UI in `localStorage`).
* `update_install` checks again with the same rules and installs only if the
  installable version equals `version`. While downloading it emits
  **`update-progress`** events `{ downloaded: number, total: number | null, finished: boolean }`
  (`total` from `Content-Length`; `finished: true` once the download is verified
  and installation starts). The signature is verified against the public key
  before anything is installed. On macOS and Linux (AppImage) the app then
  relaunches; on Windows the installer closes the app and restarts it. Refused in
  debug builds.

### `plot_2d` binary layout (little-endian)

Returned as `tauri::ipc::Response` (raw bytes); on the frontend `invoke` yields an `ArrayBuffer`.

```
u32  magic = 0x31504643  ("CFP1")
u32  header_len
u8[header_len]  header JSON: { x_label, y_label, x_ticks: Tick[], y_ticks: Tick[], total, max_count, bins,
                               x_range: [lo, hi], y_range: [lo, hi] }
u32  bins
u32[bins*bins]  counts   (index = y*bins + x; origin bottom-left; the grid covers x_range × y_range,
                          [-0.02, 1.02] on both axes when no range was requested)
u32  n
f32[n] x        (display space, already clamped into the window)
f32[n] y
u8[n]  d        (0..255 log-density rank of the point's bin, for pseudo-colour)
```

`maxPoints` caps `n` by even sub-sampling of the events inside the window; the
density grid always uses all of them. `total` is the number of events binned
(= inside the window).

### Zoomed views

`plot_2d` (`xRange`, `yRange`) and `plot_hist` (`range`) take an optional
display-space window `[lo, hi]`; omitted, invalid (non-finite, `hi <= lo`)
windows mean the full axis `[-0.02, 1.02]`. The backend re-bins the events of
the window at the requested `bins`, so a zoomed view has full resolution, and
echoes the window it used (`x_range` / `y_range` in the `plot_2d` header,
`range` in the `plot_hist` result). Events beyond a window edge that coincides
with an edge of the full axis are piled into the edge cell (as un-zoomed);
beyond an inner edge they are outside the view and not counted. Ticks
(`x_ticks`, `y_ticks`, `ticks`) are those inside the window; when fewer than
two labelled ticks would remain, round data values ("nice" steps, e.g. `20K`,
`40K`) are generated instead so a zoomed axis stays readable. The graph window
keeps the window per pair of axes; batch thumbnails, layout panels and SVG
export always use the full range.

### Population export

`export_population` writes the events of `gateId` (`null` = all events) of a
sample; `channels: null` (or `[]`) means every channel, `compensated` /
`transformed` pick the values (transformed = display space, `0..1` spans the
axis). `format` (default `"csv"`):

* `"csv"` — header row of channel names, one row per event.
* `"xlsx"` — Excel workbook with two sheets. `metadata`: rows `Field | Value`
  with `sample`, `sample_file`, `population` (gate path), `events`,
  `sample_events`, `workspace`, `exported_at` (UTC ISO 8601), `exported_by`,
  `compensation` (none / FCS `$SPILLOVER` / custom matrix channels),
  `compensated`, `values` (linear or transformed) and one `transform:<channel>`
  row per exported channel (the transform JSON), then a blank row and every FCS
  TEXT keyword of the sample (`FCS keyword | Value`). `data`: header row of
  channel names, one row per event. Populations above Excel's limit of
  1,048,575 data rows continue on `data_2`, `data_3`, ...
* `"parquet"` — Apache Parquet (Snappy), one non-null `FLOAT` column per
  channel, row groups of 1,048,576 events. The metadata fields above are stored
  as file key/value metadata, the FCS keywords prefixed `fcs:` (e.g. `fcs:$CYT`).

## File dialogs

The frontend uses `@tauri-apps/plugin-dialog` (`open`/`save`) and passes the chosen
paths to the commands above. The backend only needs `dialog:default` and the
`core:default` permissions; file reading is done in Rust, not via the fs plugin.

## Workspace file format

`*.cfw.json` — the `serde_json` of `Workspace`: version 2 stores `samples`,
`channel_settings`, `sample_channel_settings`, `compensation`, `groups`, `sample_gates`,
`layouts` and `ui_state` (no top-level `gates`). `layouts` and `ui_state` are
opaque to the core (the frontend owns their shape) and default to `[]` /
`null` for files written before they existed. Version-1 files (a single shared `gates`
list) are still read: the list becomes the "All Samples" template and is copied
into every sample. Sample paths are stored absolute; on open the core also
looks for the file name next to the workspace file.

`workspace_import_wsp` keeps each sample's own tree and transforms (as
overrides of the defaults taken from the sample with the largest tree), gives
the same population path the same gate id everywhere, and imports `<Groups>`
with their `<SampleRefs>` and template `<Subpopulations>`. Samples whose FCS
file is missing are still listed (not in `loaded`).
