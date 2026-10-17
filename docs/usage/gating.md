# Gating

A gate selects the events of a population that fall inside a shape on a plot. Gates form a hierarchy: a gate drawn while you look at a population defines a child population of it, so a population contains only the events inside *every* gate on its path from "All events".

## Where a New Gate Goes

- **Parent**: the population currently plotted (shown in the header, e.g. `Lymphocytes / Single cells`). Select "All events" in the tree to draw a top-level gate.
- **Channels**: the X and Y channels of the plot, with the compensation state of each axis (a gate drawn on `Comp-PE-A` uses compensated PE-A values).
- **Scope**: the selected group (the gate is copied into all of its samples) or the selected sample only. The scope pill in the toolbar shows which; see [Workspace and samples](workspace.md#gating-scope).

Gate coordinates are stored in display space, after the axis transform, so a polygon with straight edges on screen keeps them. If you later change a channel's scale, the gate is re-mapped so it keeps selecting the same events.

## Gate Tools

Select a tool in the graph header or press its key. Tools that need two axes are disabled on histograms, and the range tool is disabled on 2-D plots.

| Tool | Key | How to draw |
|---|---|---|
| Select / edit | **V** | Click a gate to select it; drag it or its handles (see [Editing gates](#editing-gates)) |
| Rectangle | **R** | Press and drag from one corner to the opposite one |
| Polygon | **P** | Click to place each vertex; close by clicking the first vertex, double-clicking, or pressing **Enter** (at least 3 vertices) |
| Lasso | **L** | Press the left button and trace around the events; releasing the button closes the shape |
| Ellipse | **E** | Press and drag the bounding box of the ellipse |
| Range | **G** | On a histogram, drag between the two bounds |
| Quadrants | **Q** | Click the centre point; four linked gates are created |

While drawing, press **Esc** or right-click to cancel. A rectangle, ellipse or range smaller than a few pixels is discarded.

### Rectangle

Selects the events between two bounds on each axis. Rectangle gates imported from FlowJo or drawn to the edge of the plot may have *open* bounds (no lower or upper limit), which also include events piled up on the axis.

### Polygon and lasso

A polygon is a closed shape of straight edges. The lasso is a freehand polygon: CytoFlow.rs records the path you trace and stores it as a polygon gate, so a lasso gate behaves exactly like a polygon afterwards (same editing, same statistics, same export). Use the lasso for quick irregular shapes and the polygon tool when you want control over every vertex.

### Ellipse

Drag a box: the ellipse fills it, aligned with the axes. Rotated ellipses come from FlowJo or Gating-ML imports.

### Range

On a histogram, a range gate selects the events between two values of the X channel. It is drawn as a band over the full height of the histogram.

### Linked quadrants

The quadrant tool creates four gates named after the channels and their signs, for example `Q1: CD4+ CD8+`, `Q2: CD4- CD8+`, `Q3: CD4- CD8-` and `Q4: CD4+ CD8-`. They share one centre:

- with the select tool, drag the centre point to move both dividers;
- drag the vertical or horizontal divider line to move only that one.

All four gates move together, so they always cover the plane without gaps or overlaps. Each quadrant is an ordinary population in the tree that you can rename, recolour, gate inside, export, and use in Boolean gates.

## Naming Gates

When you finish a shape, a dialog asks for the population name, pre-filled with the next free default ("Gate 1", "Gate 2", …). Type a name and press **Enter**; **Cancel** discards the gate. Names may contain any characters, including spaces, `+` and `/`, and need not be unique (statistics and exports use the full path, e.g. `Lymphocytes / Single cells / CD3+`).

To rename later: double-click the population in the tree, use **Aa** above the tree, or right-click › **Rename…** in the tree or on the plot.

## Editing Gates

With the select tool (**V**):

- **Click** a gate drawn on the plot to select it (it is drawn thicker and handles appear).
- **Drag** inside it to move it.
- **Drag a handle** to reshape it: rectangle corners, the ends of the two ellipse axes, polygon vertices, range bounds.
- **Double-click** a gate to open that population (plot its events).
- **Delete** / **Backspace** deletes the selected gate (after confirmation).
- **Esc** deselects.

Edits are saved when you release the mouse button. Editing on a group changes the gate in every member sample that has not tailored it; editing on a sample tailors it (see [Tailored gates](workspace.md#tailored-gates)).

The plot shows the child gates of the current population whose channels match the plot's axes (in either order), each labelled with its name and % of parent, and the outline of the current population's own gate as a faint dashed line. Negated gates are drawn dashed.

### Right-click menu

Right-click a gate on the plot for:

- **Show "*name*"**: plot that population
- **Rename…**, **Change colour…**
- **Negate (NOT)**: invert the gate (select the events *outside* it); the population is shown as "*name* (NOT)"
- **Delete**

Right-click an empty part of the plot to go **Back to parent population**.

### Colours

Every gate gets a colour from a palette by default. Change it with **●** above the tree or **Change colour…**: pick a swatch, a custom colour, or **×** to return to the automatic colour. The colour is used for the gate outline, its label, and its row in the tree and statistics table.

## Boolean Gates

Boolean gates combine existing populations. Click **Boolean…** in the graph header, **∩** above the tree, or right-click › **Boolean gate…**:

1. Enter a name
2. Choose the operation:
    - **AND**: events in all of the selected populations
    - **OR**: events in any of the selected populations
    - **NOT**: events in none of the selected populations
3. Tick the populations to combine (at least two for AND / OR, at least one for NOT)
4. Click **Create**

The new population is created under the population currently plotted. Boolean gates have no shape on the plot; they appear in the tree and in the statistics like any other population, and can be parents of further gates.

## Deleting Gates

Delete a gate with **✕** above the tree, **Delete** / **Backspace**, or **Delete** in a right-click menu. Its sub-populations are deleted too. With a group selected, the gate is removed from the group and from all of its samples; with a sample selected, from that sample only ("Delete from this sample").
