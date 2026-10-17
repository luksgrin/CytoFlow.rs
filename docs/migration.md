# Coming from Rodrigolab's Flow Cytometry Analyzer

[Rodrigolab's Flow Cytometry Analyzer](https://github.com/luksgrin/flow-cytometry-analyzer) is deprecated and replaced by CytoFlow.rs. This page is for people who used it: it maps every feature of the old tool to CytoFlow.rs, walks through the old workflow in the new application, and lists what is new and what behaves differently.

The short version: you still load an FCS file, draw freehand selections on scatter plots, combine them, and export the remaining events as CSV, Excel or Parquet. The difference is that your selections are now **named gates in a hierarchy** instead of a flat list of intersected filters, and they are saved in a workspace file you can reopen.

## Installing

Install CytoFlow.rs from the [Releases page](https://github.com/luksgrin/CytoFlow.rs/releases) as described in the [Installation Guide](installation.md). The two applications can be installed side by side. On macOS, the Gatekeeper step uses the new app name:

```bash
xattr -cr /Applications/CytoFlow.app
```

When you no longer need the old tool, uninstall it as before (drag "Rodrigolab's Flow Cytometry Analyzer" to the Trash, Settings > Apps on Windows, or `sudo apt-get remove flow-cytometry-analyzer` on Linux).

## Feature Mapping

| Flow Cytometry Analyzer | CytoFlow.rs | Notes |
|---|---|---|
| **Load FCS File** button | **Add FCS…** (++cmd+o++ / ++ctrl+o++) | Several files can be loaded at once and kept side by side in the **Samples** list |
| Clickable channel list; first click = X, second = Y | **X** and **Y** channel menus above the plot | Stain names are shown, e.g. `CD4 (PE-A)`. Choose **Histogram** in the Y menu for a 1-D plot |
| Lasso selection (left-click and drag) | **Lasso** tool (**L**): left-click and drag, release to close | Stored as a polygon gate. The **Polygon** tool (**P**) places vertices one click at a time |
| Selected points highlighted in green | The gate outline is drawn on the plot with its name and % of parent | Points are not recoloured; the population's events are plotted when you open it |
| **Confirm Selection** | Finishing a shape asks for a population name; **Enter** creates the gate | There is no separate confirm step. Cancel the name dialog (or press **Esc** while drawing) to discard |
| **Clear Selection** | **Esc** while drawing; **Delete** to remove a created gate | |
| Multiple filters, automatically intersected | Gate hierarchy: draw the next gate **inside** the previous population (child gates) | A population contains only events inside every gate on its path, which is the same intersection. Sibling gates are *not* intersected; use a Boolean **AND** gate to intersect populations from different branches |
| Filter per channel pair; "existing filter" warning and **Reset Filter** | Any number of gates per channel pair and per population | Edit a gate by dragging it or its handles (select tool, **V**); delete it with **Delete** |
| Filter summary (total / passing / excluded events) | **Statistics** panel: count, % of parent, % of total for every population; counts in the graph header | Excluded events = parent count − population count |
| "Show excluded points from past filters" | **Show parent events** checkbox | Draws, in grey, the parent population's events that the current gate leaves out |
| Log scale / linear scale checkboxes | Scale menu per axis: linear, log, asinh, **logicle**, **biex** | Logicle and biex show negative (compensated) values, which log axes cannot. Changing a scale keeps the gates' meaning |
| Zoom: mouse wheel; pan: right-click and drag | Same gestures; also ++space++ + drag to pan | **Reset** button → press **0**. There are no **+** / **−** zoom buttons |
| **Export Filtered Data** → CSV / Excel / Parquet | **Export population…** → CSV / XLSX / Parquet | Exports the *selected* population (open the last gate first). The XLSX file has the same **metadata** + **data** sheets. New options: compensated or raw values, display-scaled values, only the plotted channels |
| Themes: System, Light, Dim, Dark | Theme button: **Auto**, **Light**, **Dim**, **Dark** | "System" is now called "Auto"; the button cycles through the four |
| Filters lost when the app is closed | Workspace file (`*.cfw.json`, ++cmd+s++) | Reopen it later with **Open…** to get every gate back |

## A 5-Minute Walkthrough

This reproduces the typical old workflow (load, select live cells on scatter, then a fluorescent population, export) in CytoFlow.rs.

1. **Load the file.** Click **Add FCS…** and choose your FCS file. Click it in the **Samples** list. The plot shows two channels, for example FSC-A against SSC-A.

2. **Pick the channels.** In the **X** menu choose `FSC-A`, in the **Y** menu `SSC-A`. Both are on a linear scale by default, as in the old tool.

3. **Draw the first selection.** Press **L** (or click the lasso button in the graph header). Hold the left mouse button and trace around the cells; release to close the shape.

4. **Confirm it.** A dialog asks for a name: type `Cells` and press **Enter**. This is the old "Confirm Selection". The new population is selected straight away and the plot now shows only its events.

5. **Second selection on other channels.** Change **X** to a fluorescence channel, e.g. `CD3 (FITC-A)`, and **Y** to `CD4 (PE-A)`. Their scale is **logicle** (choose **log** in the scale menu if you prefer the old look; events at or below zero are then piled on the axis). Lasso the population you want and name it `CD3+ CD4+`.

    This is the old "multiple filters": because you drew it while looking at `Cells`, the new gate only contains events that are inside *both* selections. Look at the **Populations** tree: `CD3+ CD4+` is indented under `Cells`.

6. **See what was excluded.** Tick **Show parent events** to see, in grey, the `Cells` events that fall outside your new gate. The **Statistics** panel shows the count and % of parent of each population.

7. **Export.** With `CD3+ CD4+` selected, click **Export population…**, choose **CSV**, **Excel (XLSX)** or **Parquet**, keep **Compensated values** as you like, click **Export…** and choose a file name. You get the same file as with the old "Export Filtered Data": the events passing all of your selections.

8. **Save.** Press ++cmd+s++ (++ctrl+s++) to save the workspace. Next time, **Open…** it and all your gates are back; you can also load more FCS files and the same gates are applied to them.

## What Is Not Identical

- **No "Confirm Selection" button.** The gate is created when you name it. Pressing **Esc** while drawing, or cancelling the name dialog, discards the shape.
- **Selections are not intersected automatically across branches.** Only gates drawn *inside* another population (children) combine. If you draw two gates while looking at the same population, they are two separate (sibling) populations. To intersect them, create a Boolean **AND** gate (**Boolean…**).
- **The export takes one population.** The old tool exported the events passing all filters; CytoFlow.rs exports the population you select. Select the deepest gate of your chain to get the same result.
- **Excel export**: the **metadata** sheet lists the FCS keywords and the export settings; the layout of the sheets may differ in detail from the old tool's. Excel's row limit (1,048,576 rows) applies; use CSV or Parquet for bigger populations.
- **Log scale**: the old log checkbox was a plain log10 axis. CytoFlow.rs' **log** scale has a fixed number of decades (**M**, default 4.5) below the top of the scale (**T**), so very small values are clipped at the bottom. For compensated fluorescence, use **logicle** or **biex** instead.
- **Default scales**: fluorescence channels open on logicle, not linear.
- **Selected points are not recoloured green**; instead the gate is outlined, and opening the population shows only its events.
- **Zoom buttons**: the **+** / **−** buttons are gone; use the mouse wheel, and **0** to reset.
- **Lasso shape**: the freehand path is stored as a polygon, so after drawing you can move its vertices with the select tool.

## What Is New

Things the old tool could not do:

- **Gate hierarchy** with named populations, plus rectangle, polygon, ellipse, range, linked quadrant and Boolean (AND / OR / NOT) gates, and negated gates
- **Editing gates** after drawing them (move, reshape, rename, recolour)
- **Several samples** in one workspace, **groups** that apply the same gates to many samples, and per-sample **tailored** gates
- **Compensation** from the FCS file's spillover matrix or a custom matrix
- **Logicle, biex and arcsinh** scales with editable parameters
- **Density and contour** plots, **histograms**, **overlays** of populations or samples
- **Statistics**: % of parent / % of total for every population, median / mean / CV / SD / geometric mean per channel, and a statistics table export for all samples
- **Batch view** of the same plot across all samples, a **layout editor** for figures, PNG/SVG export and copying plots to the clipboard
- **FlowJo `.wsp` import** with counts that match FlowJo (see [Validation](validation.md))
- **Workspace files** that keep everything between sessions
- **Keyboard shortcuts** for every tool (see [Keyboard shortcuts](usage/shortcuts.md))

## Getting Help

If something you relied on in the old tool is missing or behaves unexpectedly, open an [issue on GitHub](https://github.com/luksgrin/CytoFlow.rs/issues) and mention that you are coming from Flow Cytometry Analyzer.
