# Test data

All files in this directory are public test fixtures taken from the
[FlowKit](https://github.com/whitews/FlowKit) project (BSD‑3‑Clause,
Copyright (c) 2018, Scott White). They are used only to validate the
FCS parser, the Gating‑ML reference gates and the FlowJo `.wsp` importer.

* `gate_ref/data1.fcs` + `gate_ref/gml/*.xml` + `gate_ref/truth/*.txt`:
  the Gating‑ML 2.0 compliance set (per‑event 0/1 membership truth files).
* `test_comp_example.fcs`: FCS 3.0 file with a `$SPILL` matrix.
* `wsp/*.wsp`: FlowJo 10 workspaces used by the importer tests.
* `large/`: not committed. Run `scripts/fetch-testdata.sh` to download the
  8‑colour ICS FCS files (~17 MB each) that the `.wsp` files reference.

Nothing in this repository is derived from FlowJo binaries.
