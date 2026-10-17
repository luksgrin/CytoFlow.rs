# Statistics and Export

## Population Statistics

The **Statistics** table in the right column lists every population of the plotted sample's gate tree, in tree order (indented by depth, with the gate colour):

| Column | Meaning |
|---|---|
| **Population** | The population name ("All events" first) |
| **Count** | Number of events in the population |
| **% Parent** | Count as a percentage of the parent population's count |
| **% Total** | Count as a percentage of all events in the sample |

Click a row to plot that population. Tailored populations are marked. When a group is selected, populations that exist only in the sample's own tree (and not in the group template) are shown dimmed and cannot be selected from there.

The same count and % of parent appear next to each population in the **Populations** tree, on the gate labels in the plot, and in the graph header.

## Channel Statistics

Under **Selected population**, CytoFlow.rs lists statistics of the plotted channels (X, and Y for 2-D plots) for the plotted population:

| Column | Meaning |
|---|---|
| **Channel** | The channel (prefixed "Comp" when the axis uses compensated values) |
| **Median** | Median of the raw (or compensated) values |
| **Mean** | Arithmetic mean |
| **CV %** | Coefficient of variation, `100 · SD / mean` |

Hover a row to see the standard deviation (SD), geometric mean (shown only when every value is positive), minimum and maximum. Statistics are computed on the instrument values, not on the display-scaled values, so they do not depend on the axis scale.

## Exporting a Population

Use **Export population…** in the toolbar to write the events of the selected population of the plotted sample to a file.

The dialog offers:

- **Format**: **CSV**, **Excel (XLSX)** or **Parquet**
- **Compensated values**: write compensated values (ticked by default when compensation is active; scatter and Time channels are never compensated)
- **Transformed (display-space) values**: write the axis-scaled values (about 0 to 1) instead of instrument values
- **Only the channels on the current axes**: write just the X (and Y) channel instead of every channel

Click **Export…** and choose the file name. The status bar reports how many events were written.

### CSV

- One header row with the channel names (`$PnN`), then one row per event
- Can be opened in Excel, LibreOffice, Google Sheets, R, Python or any text editor

### Excel (XLSX)

- Two sheets, **metadata** and **data**
- **metadata**: the FCS file information and its keywords (TEXT segment), and the export settings (sample, population path, compensation, transformed)
- **data**: the exported events, one column per channel
- Excel worksheets hold at most 1,048,576 rows; use CSV or Parquet for larger populations

### Parquet

- Compressed columnar format, one column per channel
- Efficient for large populations
- Can be read by Python (`pandas.read_parquet`, `polars`), R (`arrow::read_parquet`) and other data analysis tools

## Exporting the Statistics Table

**Export stats…** in the toolbar (or **Export…** above the Statistics table) writes a CSV with one row per sample × population, for every loaded sample, each with its own gate tree:

| Column | Content |
|---|---|
| `sample` | Sample name |
| `population` | Full population path, e.g. `Lymphocytes/Single cells/CD3+` ("All events" for the root) |
| `count` | Number of events |
| `pct_parent` | % of parent (4 decimals) |
| `pct_total` | % of all events (4 decimals) |

This file is ready for a pivot table or for `pandas` / R to compare populations across samples.

## Exporting Images

- **Copy** in the graph header copies the current plot as a PNG (see [The graph window](graph.md#copying-a-plot))
- The [layout editor](layout.md) exports whole pages as PNG or SVG
