# Validation

A gating tool is only useful if its numbers are right. CytoFlow.rs' engine (`crates/cytoflow-core`) is tested against public ground truth: the ISAC Gating-ML 2.0 compliance set, a lookup table exported by FlowJo, and the population counts that FlowJo stores in its own workspace files. No FlowJo program or code is used; only public files produced by it.

All tests run with:

```bash
scripts/fetch-testdata.sh          # once: downloads the large public fixtures
cargo test -p cytoflow-core
```

Tests that need a large fixture which has not been downloaded are skipped with a message instead of failing. CI downloads the fixtures (and caches them), so every test runs on every push and pull request (see [Building from Source](building.md#running-the-tests)).

## Summary

| What | Reference | Result |
|---|---|---|
| Gating-ML 2.0 gates (rectangles, ranges, polygons, ellipses, quadrants, Boolean, parent gates; linear, log, asinh, logicle transforms; spectrum-matrix compensation) | ISAC compliance set: per-event membership truth files | **Exact**, every event of every supported gate |
| FlowJo `biex` transform | Lookup table exported by FlowJo (4096 channels) | **Exact** to the table's 6 significant digits |
| FlowJo rectangles and quadrants (linear, logicle, asinh axes) | Counts stored in FlowJo workspaces | **Exact** |
| FlowJo rectangles and quadrants on `biex` axes | Counts stored in FlowJo workspaces | **Exact** |
| FlowJo polygons on linear axes | Counts stored in FlowJo workspaces | **Exact** |
| FlowJo polygons on `biex` axes | FlowSOM example workspace (FlowJo 10.5.3) | Within **2 events** per population |
| FlowJo polygons on logicle axes, and populations below them | 8-colour ICS workspace (3 samples) | Within **0.1 %** of the parent population (within 0.2 % of the count for populations of 10,000+ events) |
| FlowJo ellipse on linear axes | 100 events on a line, 51 inside | **Exact** |
| FlowJo ellipse on logicle axes | 8-colour ICS workspace (3 samples) | **2.0–3.1 % under** FlowJo (known limitation) |

## Gating-ML 2.0 Conformance

`tests/gatingml_conformance.rs` reads every gate definition in `testdata/gate_ref/gml/*.xml`, evaluates it on `data1.fcs`, and compares the result **event by event** with the published `Results_<GateId>.txt` truth file. The tolerated number of mismatching events is **zero**.

A second test checks that the spillover-matrix orientation is right: reading the `spectrumMatrix` with rows = fluorochromes and columns = detectors reproduces the truth exactly, while the transposed matrix breaks agreement badly.

Gates that need a feature the engine deliberately does not implement are skipped explicitly (listed in the test, never silently):

| Gate ids | Reason |
|---|---|
| `RatRange1`, `RatRange1a`, `RatRange2` | `fratio` (ratio / new-dimension) dimensions are not supported |
| `ScaleRange2`, `ScaleRange2c`, `ScaleRange7c`, `ScalePar1` | `hyperlog` transform is not supported |
| `Ellipsoid3D` | 3-D ellipsoids are not supported |

## FlowJo Biexponential Scale

`tests/biex_flowjo_table.rs` compares the analytic `biex` transform with a table exported by FlowJo itself (`testdata/flowjo_xforms/`, from FlowKit): for width basis −7.943282, 1 extra negative decade, 4.418540 positive decades and a range of 262144, each of the 4096 channels maps to a listed raw value. Every entry is reproduced in both directions, to the 6 significant digits the table is printed with.

`tests/wsp_biex_polygon.rs` then checks that the exact shape of the scale matters: polygons drawn on two `biex` axes (width −100) in the FlowSOM example workspace agree within 2 events, whereas the earlier logicle approximation of `biex` picked up 7 extra events in one of them.

## FlowJo Workspace Counts

`tests/wsp_import.rs` imports public FlowJo 10 workspaces, recomputes every population of every sample whose FCS file is present, and compares each count with the count FlowJo stored in the workspace. The fixtures are:

| Workspace | Content | Tolerance |
|---|---|---|
| `8_color_ICS_simple.wsp` | 3 samples, polygons and rectangles on logicle axes, compensation, per-sample Time gain | 0.1 % (Time gate exact in every sample) |
| `8_color_ICS_with_ellipse.wsp` | The same with an ellipse on logicle axes | 0.1 % of parent; the ellipse 3.5 % |
| `reused_quad_gate_with_child.wsp` | Quadrants with a child gate on `biex` axes | Exact |
| `test_data_diamond_biex_rect.wsp` | Rectangle on `biex` axes | Exact |
| `test_data_diamond_asinh_rect.wsp`, `…_rect2.wsp` | Rectangles on asinh axes | 0.1 % |
| `simple_diamond_example_quad_gate.wsp` | Quadrant gate | 0.1 % |
| `single_ellipse_51_events.wsp` | Ellipse on linear axes over 100 events | Exact (51 events) |
| FlowSOM `gating.wsp` | Polygons on `biex` axes | 2 events |

Two FlowJo conventions had to be pinned down to get exact counts, and each has its own test:

- **Polygons** are evaluated on events snapped to FlowJo's display grid (`gateResolution`, usually 256 steps). With the snapping, polygons on linear axes match exactly; on logicle axes a few events per thousand near the boundary can still flip.
- **Rectangles drawn to the edge of the axis** (`tests/wsp_rect_edge.rs`): a bound that falls inside the first or last cell of the 256-step display grid is treated as open, which is what makes the Time gate of every sample of `8_color_ICS_simple.wsp` exact. The test documents the alternatives tried and the counts each one breaks.

The test also checks that a workspace imports correctly when none of its FCS files are present (samples are listed as not loaded, trees kept).

## Known Limitations

- **Ellipses on logicle axes count 2–3 % fewer events than FlowJo.** The ellipse geometry (centre at the midpoint of the foci, semi-axes from the edge points, coordinates on the 256-step grid) is exact on linear axes, but on the logicle axes of `8_color_ICS_with_ellipse.wsp` it gives 5875 vs 6002 (E03), 6010 vs 6133 (E05) and 7032 vs 7256 (E01) events. `tests/wsp_ellipse_explore.rs` tabulates the candidate display conventions that were tried (FlowJo's `biex` in place of logicle, 4096/1024/256-channel tables, rounding or flooring events, a bin-aligned logicle zero, a grid spanning the data range, polygonal ellipses); none closes the gap without a fitted constant, so none is applied. About 10–15 events of each deficit are inherited from the parent polygons. Run the exploration with:
    ```bash
    cargo test --release -p cytoflow-core --test wsp_ellipse_explore -- --ignored --nocapture
    ```
- **Gating-ML**: the `hyperlog` transform, ratio dimensions (`fratio`) and 3-D (or higher) ellipsoids are not supported. In FlowJo imports, a `hyperlog` axis is approximated by a logicle with the same parameters.
- **FlowJo CurlyQuad gates** (and any other gate type not listed in [Importing FlowJo workspaces](usage/flowjo-import.md)) are not imported; the population and its sub-populations are skipped with a warning.
- **Only one compensation matrix** per FlowJo workspace is used.
- **ASCII FCS data** (`$DATATYPE/A`) and integer data whose bit width is not a multiple of 8 are not supported.
- **The application bundles are unsigned**: macOS Gatekeeper and Windows SmartScreen warn on first launch (see [Installation](installation.md)).

## Test Data

All fixtures are public:

- `testdata/gate_ref/`, `testdata/wsp/`, `testdata/flowjo_xforms/`, `testdata/test_comp_example.fcs`: from [FlowKit](https://github.com/whitews/FlowKit) (BSD-3-Clause), committed
- `testdata/large/`: not committed, downloaded by `scripts/fetch-testdata.sh`: the 8-colour ICS FCS files (~17 MB each) and the diamond example from FlowKit, and `gating.wsp` + `68983.fcs` from the [FlowSOM](https://github.com/SofieVG/FlowSOM) R package (GPL ≥ 2), downloaded rather than committed so that the committed fixtures stay under the BSD licence

See `testdata/README.md` in the repository for details.
