# Test data

All files in this directory are public test fixtures taken from the
[FlowKit](https://github.com/whitews/FlowKit) project (BSD‑3‑Clause,
Copyright (c) 2018, Scott White). They are used only to validate the
FCS parser, the Gating‑ML reference gates and the FlowJo `.wsp` importer.

* `gate_ref/data1.fcs` + `gate_ref/gml/*.xml` + `gate_ref/truth/*.txt`:
  the Gating‑ML 2.0 compliance set (per‑event 0/1 membership truth files).
* `test_comp_example.fcs`: FCS 3.0 file with a `$SPILL` matrix.
* `wsp/*.wsp`: FlowJo 10 workspaces used by the importer tests.
* `wsp/single_ellipse_51_events.wsp` + `wsp/data_set_simple_line_100.fcs`:
  a FlowJo ellipse on linear axes over 100 events on a line (FlowKit
  `data/simple_line_example`); pins the ellipse convention.
* `flowjo_xforms/tr_biex_l256_w-7.943282_n1.000000_m4.418540_r262144.000029.csv`:
  a biexponential lookup table exported by FlowJo (channel → raw value,
  FlowKit `data/flowjo_xforms`), used to validate the `biex` transform.
* `large/`: not committed. Run `scripts/fetch-testdata.sh` to download the
  8‑colour ICS FCS files (~17 MB each) that the `.wsp` files reference, and
  `large/flowsom/{gating.wsp,68983.fcs}` from the
  [FlowSOM](https://github.com/SofieVG/FlowSOM) R package (`inst/extdata`,
  GPL ≥ 2, Van Gassen et al.): a FlowJo 10.5.3 workspace with polygons on
  biex axes, used by `tests/wsp_biex_polygon.rs`. It is downloaded rather
  than committed, so that the committed fixtures stay under the BSD licence.

Nothing in this repository is derived from FlowJo binaries.
