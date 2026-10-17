# CytoFlow.rs

A free, open-source flow cytometry analysis application for macOS (and Linux /
Windows), written in Rust with a Tauri 2 desktop shell. It covers the everyday
FlowJo-style workflow: load FCS files, compensate, pick axis scales, draw a
hierarchical gate tree, read population statistics, and export.

CytoFlow.rs is a **clean-room implementation**. It is built only from public
specifications and papers (ISAC FCS 3.1, Gating-ML 2.0, the logicle scale of
Parks, Roederer & Moore 2006, and the openly documented FlowJo `.wsp` XML
layout) and is validated against public ground-truth fixtures. It contains no
code or assets derived from any commercial product.

## Features

* **FCS reader**: FCS 2.0 / 3.0 / 3.1, integer / float / double data, both byte
  orders, per-parameter bit widths, `$PnE` log amplification, `$PnG` gain,
  `$SPILLOVER` / `SPILL` / `$COMP` matrices.
* **Scales**: linear, log, arcsinh and logicle (biexponential), with automatic
  logicle width estimation and editable parameters. Changing a scale re-maps
  existing gates so they keep their meaning.
* **Compensation**: from the file's own matrix, none, or a custom editable matrix.
* **Gating**: rectangle, range, polygon, ellipse, quadrant and Boolean (and / or /
  not) gates in a population hierarchy shared across samples.
* **Plots**: pseudo-colour dot plot, density, contour and histogram, with
  proper decade tick marks on log-like axes; a batch grid across samples.
* **Statistics**: count, % of parent, % of total, per-channel mean / median /
  CV / SD / geometric mean; CSV export of populations and statistics tables.
* **FlowJo `.wsp` import**: samples, transforms, compensation matrix and the
  gate tree (rectangle / quadrant, polygon, ellipsoid, Boolean nodes).
  FlowJo's `biex` scale is approximated by a logicle scale.
* **Workspace files**: plain JSON (`*.cfw.json`).

## Layout

```
crates/cytoflow-core   pure Rust engine (no UI), unit + conformance tests
src-tauri              Tauri 2 shell exposing the engine as IPC commands
src                    Vite + TypeScript user interface
docs/ipc.md            the frontend ⇄ backend contract
testdata               public fixtures (FlowKit, BSD-3) for the test suite
```

## Build

Requirements: Rust (stable), Node ≥ 20, `cargo install tauri-cli --version "^2"`,
and on macOS the Xcode command-line tools.

```sh
npm install
cargo test -p cytoflow-core         # engine tests (Gating-ML conformance, wsp import)
cargo tauri dev                     # run the app with hot reload
cargo tauri build                   # produce CytoFlow.app and a .dmg in src-tauri/target/release/bundle
```

Large test fixtures are not committed; `scripts/fetch-testdata.sh` downloads them.

Files can also be opened from the command line (FCS files, `*.cfw.json`
workspaces or FlowJo `.wsp` files):

```sh
open target/release/bundle/macos/CytoFlow.app --args /path/to/sample.fcs /path/to/other.fcs
```

## Status

Early but usable. The engine is validated against the Gating-ML 2.0 compliance
fixtures and against population counts stored in FlowJo workspaces. See
`docs/ipc.md` for the command surface, and open an issue for missing features.

## License

MIT. Test fixtures under `testdata/` are from FlowKit (BSD-3-Clause).
