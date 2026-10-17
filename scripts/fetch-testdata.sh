#!/usr/bin/env bash
# Downloads the large public FCS fixtures (FlowKit, BSD-3) used by the
# workspace-import integration tests. Safe to re-run.
set -euo pipefail
mkdir -p "$(dirname "$0")/../testdata/large"
cd "$(dirname "$0")/../testdata/large"
BASE="https://raw.githubusercontent.com/whitews/FlowKit/master/data"
for f in 8_color_data_set/fcs_files/101_DEN084Y5_15_E01_008_clean.fcs \
         8_color_data_set/fcs_files/101_DEN084Y5_15_E03_009_clean.fcs \
         8_color_data_set/fcs_files/101_DEN084Y5_15_E05_010_clean.fcs \
         simple_diamond_example/test_data_diamond_01.fcs; do
  name=$(basename "$f")
  [ -f "$name" ] || { echo "fetching $name"; curl -fsSL -o "$name" "$BASE/$f"; }
done
# FlowSOM example data (GPL >= 2): FlowJo workspace with polygons on biex axes.
mkdir -p flowsom
FLOWSOM="https://raw.githubusercontent.com/SofieVG/FlowSOM/master/inst/extdata"
for name in gating.wsp 68983.fcs; do
  [ -f "flowsom/$name" ] || { echo "fetching flowsom/$name"; curl -fsSL -o "flowsom/$name" "$FLOWSOM/$name"; }
done
echo done
