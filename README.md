# CytoFlow.rs

[![CI](https://github.com/luksgrin/CytoFlow.rs/actions/workflows/ci.yml/badge.svg)](https://github.com/luksgrin/CytoFlow.rs/actions/workflows/ci.yml)
[![Docs](https://github.com/luksgrin/CytoFlow.rs/actions/workflows/docs.yml/badge.svg)](https://luksgrin.xyz/CytoFlow.rs/)
[![Release](https://img.shields.io/github/v/release/luksgrin/CytoFlow.rs?include_prereleases&sort=semver)](https://github.com/luksgrin/CytoFlow.rs/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A free, open-source flow cytometry analysis application for macOS, Windows and Linux. It covers the everyday FlowJo-style workflow: load FCS files, compensate, pick axis scales, draw a hierarchical gate tree, read population statistics, and export.

Built with Rust (the `cytoflow-core` analysis engine) and Tauri 2, with a TypeScript user interface.

**Documentation: <https://luksgrin.xyz/CytoFlow.rs/>**

![CytoFlow.rs main window](docs/img/main-light.png)

> **Coming from Rodrigolab's Flow Cytometry Analyzer?** That tool is deprecated in favour of CytoFlow.rs. See [Coming from Flow Cytometry Analyzer](https://luksgrin.xyz/CytoFlow.rs/migration/) for a feature-by-feature mapping and a 5-minute walkthrough.

## Features

- **FCS reader**: FCS 2.0 / 3.0 / 3.1, integer / float / double data, both byte orders, per-parameter bit widths, `$PnE` log amplification, `$PnG` gain, `$SPILLOVER` / `SPILL` / `$COMP` matrices
- **Scales**: linear, log, arcsinh, logicle and a FlowJo-compatible biexponential (`biex`, verified against a FlowJo-exported lookup table), with automatic logicle width estimation and editable parameters. Changing a scale re-maps existing gates so they keep their meaning
- **Compensation**: from the file's own matrix, none, or a custom editable matrix
- **Gating**: rectangle, range, polygon, freehand lasso, ellipse, linked quadrant and Boolean (AND / OR / NOT) gates in a population hierarchy; negated gates; editable after drawing
- **Groups**: sample groups carry template gate trees (the built-in "All Samples" group contains every sample); each sample owns its own tree, which can be tailored per sample and pushed back to a group. Transforms can be overridden per sample
- **Plots**: pseudocolour dot plot, density, contour and histogram, with decade tick marks on log-like axes; wheel zoom and drag pan; "show parent events"; overlays of several populations or samples; a batch grid across samples; copy as PNG
- **Layout editor**: arrange plots, histograms and statistics tables on A4 / Letter pages, re-rendered from live data, exported as PNG or SVG
- **Statistics**: count, % of parent, % of total, per-channel mean / median / CV / SD / geometric mean
- **Export**: population events as CSV, Excel (XLSX, with `metadata` and `data` sheets) or Parquet; statistics tables as CSV
- **FlowJo `.wsp` import**: samples, groups, per-sample transforms, compensation matrix and each sample's own gate tree (rectangle / quadrant, polygon, ellipsoid, Boolean nodes)
- **Workspace files**: plain JSON (`*.cfw.json`), including named layouts and the last view of each sample
- **Themes**: Auto, Light, Dim and Dark
- **Keyboard shortcuts** for every gate tool

## Installation

### Pre-built Binaries (Recommended)

Download pre-built binaries from the [GitHub Releases](https://github.com/luksgrin/CytoFlow.rs/releases) page. Every release provides:

- **macOS**: `CytoFlow_<version>_aarch64.dmg` (Apple Silicon) and `CytoFlow_<version>_x64.dmg` (Intel)
- **Windows**: `CytoFlow_<version>_x64_en-US.msi` (MSI) and `CytoFlow_<version>_x64-setup.exe` (per-user installer, no administrator rights)
- **Linux**: `CytoFlow_<version>_amd64.deb` (Debian / Ubuntu) and `CytoFlow_<version>_amd64.AppImage` (portable)
- `source.tar.gz`

The builds are not code-signed:

**Note for macOS users:** If you see an error that the app is "damaged and can't be opened", this is due to macOS Gatekeeper. After dragging the app to Applications, run `xattr -cr /Applications/CytoFlow.app`, or right-click the app and choose "Open". See the [Installation Guide](https://luksgrin.xyz/CytoFlow.rs/installation/#macos) for all three methods.

**Note for Windows users:** Windows SmartScreen may show a warning when installing. This is normal for unsigned applications. Click "More info" then "Run anyway" to proceed. See the [Installation Guide](https://luksgrin.xyz/CytoFlow.rs/installation/#windows) for details.

**Linux:** `sudo apt install ./CytoFlow_<version>_amd64.deb`, or `chmod +x` the AppImage and run it.

### Building from Source

See [Building from Source](https://luksgrin.xyz/CytoFlow.rs/building/) for detailed instructions, or follow the quick start below.

## Quick Start

1. Launch CytoFlow and click **Add FCS…** (⌘O / Ctrl+O) to load one or more FCS files
2. Click a sample, then choose the **X** and **Y** channels and their scales above the plot
3. Pick a gate tool (**R** rectangle, **P** polygon, **L** lasso, **E** ellipse, **Q** quadrants, **G** range) and draw on the plot; name the population and press Enter
4. Click the new population in the tree to plot its events and gate further inside it
5. Read the counts and percentages in the **Statistics** panel
6. **Export population…** writes the selected population as CSV, XLSX or Parquet; **Export stats…** writes the statistics table
7. Save the workspace with ⌘S / Ctrl+S

Files can also be opened from the command line (FCS files, `*.cfw.json` workspaces or FlowJo `.wsp` files):

```sh
open /Applications/CytoFlow.app --args /path/to/sample.fcs /path/to/other.fcs
```

See the [User Guide](https://luksgrin.xyz/CytoFlow.rs/usage/) for everything else.

## Documentation

The full documentation is at **<https://luksgrin.xyz/CytoFlow.rs/>**:

- [Installation](https://luksgrin.xyz/CytoFlow.rs/installation/)
- [User Guide](https://luksgrin.xyz/CytoFlow.rs/usage/) and [Keyboard shortcuts](https://luksgrin.xyz/CytoFlow.rs/usage/shortcuts/)
- [Coming from Flow Cytometry Analyzer](https://luksgrin.xyz/CytoFlow.rs/migration/)
- [Validation](https://luksgrin.xyz/CytoFlow.rs/validation/)
- [Troubleshooting](https://luksgrin.xyz/CytoFlow.rs/troubleshooting/)

The sources are in [`docs/`](docs/); [`docs/ipc.md`](docs/ipc.md) is the frontend ⇄ backend contract.

## Building

Requirements: Rust (stable), Node ≥ 20, `cargo install tauri-cli --version "^2" --locked`, and the platform's build tools (Xcode command-line tools on macOS, Visual C++ Build Tools on Windows, WebKitGTK 4.1 development packages on Linux; see the [build guide](https://luksgrin.xyz/CytoFlow.rs/building/)).

```sh
git clone https://github.com/luksgrin/CytoFlow.rs.git
cd CytoFlow.rs
npm install
scripts/fetch-testdata.sh            # optional: large fixtures for the wsp import tests
cargo test -p cytoflow-core          # engine tests (Gating-ML conformance, wsp import)
cargo tauri dev                      # run the app with hot reload
cargo tauri build                    # installers in target/release/bundle/
```

`npm run dev` alone runs the user interface in a browser against a mock engine with synthetic data, which is handy for UI work.

### Repository layout

```
crates/cytoflow-core   pure Rust engine (no UI), unit + conformance tests
src-tauri              Tauri 2 shell exposing the engine as IPC commands
src                    Vite + TypeScript user interface
docs                   documentation (MkDocs); docs/ipc.md is the frontend ⇄ backend contract
testdata               public fixtures (FlowKit, BSD-3) for the test suite
scripts                test-data download, version check, release bundle collection
```

Releases are cut with a manual GitHub Actions workflow; see [RELEASING.md](RELEASING.md).

## Platform Support

- **macOS**: 11 (Big Sur) or later, Apple Silicon and Intel
- **Windows**: Windows 10 or later (x64)
- **Linux**: Ubuntu 22.04+, Debian 11+ (x86_64)

## Validation

The engine is validated against public ground truth (details in [Validation](https://luksgrin.xyz/CytoFlow.rs/validation/)):

- **Gating-ML 2.0** compliance fixtures: exact per-event agreement for every supported gate
- **FlowJo biexponential** scale: reproduces a FlowJo-exported lookup table
- **FlowJo workspaces**: population counts are exact for rectangle, quadrant and biex/linear-axis gates; within 0.1 % for polygons on logicle axes; ellipses on logicle axes are still 2–3 % under FlowJo (see `crates/cytoflow-core/tests/wsp_ellipse_explore.rs`)

Known gaps: ellipses on logicle axes as above; hyperlog, ratio dimensions and 3-D ellipsoids from Gating-ML; FlowJo CurlyQuad gates; ASCII FCS data; the application bundles are unsigned.

## Clean-Room Implementation

CytoFlow.rs is a **clean-room implementation**. It is built only from public specifications and papers (ISAC FCS 3.1, Gating-ML 2.0, the logicle scale of Parks, Roederer & Moore 2006, and the openly documented FlowJo `.wsp` XML layout) and is validated against public ground-truth fixtures. It contains no code or assets derived from any commercial product.

## License

MIT, Copyright (c) 2026 Lucas Goiriz. Test fixtures under `testdata/` are from [FlowKit](https://github.com/whitews/FlowKit) (BSD-3-Clause).
