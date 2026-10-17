# File Formats

## FCS Files

CytoFlow.rs reads list-mode files of the ISAC Flow Cytometry Standard, written from the FCS 3.1 specification.

### Supported

| Feature | Support |
|---|---|
| Versions | FCS 2.0, 3.0 and 3.1 (header magic `FCS2.0`, `FCS3.0`, `FCS3.1`) |
| Mode | List mode (`$MODE/L`, the default when the keyword is absent) |
| Data types (`$DATATYPE`) | `I` integer, `F` 32-bit float, `D` 64-bit double |
| Byte order (`$BYTEORD`) | Little-endian (`1,2,3,4`, also `1,2` and `1,2,3,4,5,6,7,8`) and big-endian (`4,3,2,1`, `2,1`, `8,7,6,5,4,3,2,1`) |
| Integer widths (`$PnB`) | Any multiple of 8 bits up to 64, which may differ per parameter; values are masked to the next power of two ≥ `$PnR` |
| Segments | Primary TEXT, supplemental TEXT (`$BEGINSTEXT` / `$ENDSTEXT`), DATA from the header or from `$BEGINDATA` / `$ENDDATA` (files larger than 99,999,999 bytes) |
| File extensions | `.fcs`, `.FCS`, `.lmd` (in the file dialog; the content decides) |

### Keywords used

| Keyword | Used for |
|---|---|
| `$PAR`, `$TOT` | Number of parameters and events |
| `$PnN` | Channel name (used everywhere: axes, gates, exports) |
| `$PnS` | Stain name, shown as `stain (channel)` in menus and labels |
| `$PnR` | Channel range: top of scale (**T**) of the default transforms, integer masks |
| `$PnB` | Bits per integer value |
| `$PnE` | Log amplification `f1,f2`: integer channel values are converted to `f2 · 10^(f1 · value / $PnR)` (an `f2` of 0 is read as 1) |
| `$PnG` | Gain: linear values are divided by the gain (FCS 3.1 §3.2.19) |
| `$SPILLOVER`, `SPILL`, `SPILLOVER`, `$COMP`, `COMP` | Spillover matrix for compensation (first one found, in this order) |
| `$CYT` | Cytometer name, shown under the sample name |
| `$MODE`, `$DATATYPE`, `$BYTEORD` | Data layout |

All other keywords are kept and shown in the **Keywords…** dialog, and written to the metadata sheet of XLSX exports.

### Not supported

- **ASCII data** (`$DATATYPE/A`): the file is refused with "Unsupported FCS feature: ASCII ($DATATYPE/A) data"
- **Histogram / correlated modes** (`$MODE/C`, `$MODE/U`), deprecated since FCS 3.1
- **Integer widths that are not a multiple of 8 bits** (e.g. 10- or 12-bit packed data)
- **Mixed-endian byte orders** such as `3,4,1,2`
- **Several data sets in one file** (`$NEXTDATA`): only the first is read

## Workspace Files (`*.cfw.json`)

A CytoFlow workspace is a plain JSON file, normally named `<name>.cfw.json`. It contains:

| Field | Content |
|---|---|
| `version` | Format version (currently 2) |
| `name` | Workspace name |
| `samples` | Each sample's id, name, absolute FCS path, event count, channels and keywords |
| `channel_settings` | The default transform of every channel |
| `sample_channel_settings` | Per-sample transform overrides |
| `compensation` | `none`, `fcs` (each file's own matrix) or `custom` with the matrix |
| `groups` | Groups, their member samples and template gate trees ("All Samples" first) |
| `sample_gates` | The gate tree of every sample (with the `tailored` flag) |
| `layouts` | The named layout-editor pages |
| `ui_state` | User-interface state: last axes and population per sample, overlay selection, active layout |

It does **not** contain event data: keep the FCS files. When a workspace is opened, each sample is loaded from its stored path, or, if that does not exist, from a file of the same name next to the workspace file.

Gate coordinates are stored in display space (after the axis transform). Version-1 files, which had a single shared `gates` list and no groups, are still read: the list becomes the template of "All Samples" and is copied into every sample.

The exact JSON shapes of every type (`Transform`, `GateShape`, `Group`, `WorkspaceView`, …) are documented in the [IPC contract](ipc.md), which is also the contract between the user interface and the engine.

## FlowJo Workspaces (`.wsp`)

FlowJo 10 `.wsp` files can be imported; they are not written. See [Importing FlowJo workspaces](usage/flowjo-import.md).

## Gating-ML 2.0

The engine (`cytoflow-core`) includes a Gating-ML 2.0 reader and evaluator, used to check conformance against the official ISAC compliance set (see [Validation](validation.md)). It reads:

- `RectangleGate` (including ranges and open bounds), `PolygonGate`, 2-D `EllipsoidGate`, `QuadrantGate` and `BooleanGate` (and / or / not), with parent gates;
- transformations `flin`, `flog`, `fasinh` and `logicle`;
- `spectrumMatrix` compensation (rows = fluorochromes, columns = detectors) and FCS-file compensation.

Not supported: the `hyperlog` transform, ratio dimensions (`fratio` / `new-dimension`) and ellipsoids of more than two dimensions.

The compliance fixtures live in `testdata/gate_ref/`: `data1.fcs`, the gate definitions in `gml/*.xml` and the per-event 0/1 membership truth files in `truth/Results_<GateId>.txt`. Importing Gating-ML files from the user interface is not available yet.

## Export Formats

| Export | Format |
|---|---|
| Population events | CSV, XLSX (**metadata** + **data** sheets) or Parquet; see [Statistics and export](usage/statistics-export.md#exporting-a-population) |
| Statistics table | CSV with columns `sample`, `population`, `count`, `pct_parent`, `pct_total` |
| Plots | PNG (Copy) |
| Layout pages | PNG (2×) and SVG |
