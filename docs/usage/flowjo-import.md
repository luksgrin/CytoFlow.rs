# Importing FlowJo Workspaces

CytoFlow.rs can open FlowJo 10 workspace files (`.wsp`) and rebuild their samples, groups, transforms, compensation and gate trees. The importer is written from the public structure of the `.wsp` XML (and the FlowKit / CytoML descriptions of it); it does not use anything from the FlowJo program.

## How to Import

1. Click **Import FlowJo .wsp…** in the toolbar and choose the `.wsp` file
2. CytoFlow.rs looks for each sample's FCS file:
    - at the absolute path recorded in the workspace,
    - otherwise by file name in the folder of the `.wsp` file, and one folder level below it
3. If some files are not found, the **Missing FCS files** dialog lists them (name and recorded path). Click **Locate folder…** and choose the folder that contains them (CytoFlow.rs also searches its immediate sub-folders), or **Skip** to continue without them
4. The imported workspace replaces the current one. Warnings, if any, are shown in the status bar

Samples whose file is still missing are kept in the workspace, with their gate trees, and are shown as "file not found"; their statistics are unavailable until the file is found. Save the imported workspace as a CytoFlow workspace (`*.cfw.json`) to keep working on it.

You can also pass a `.wsp` file on the command line when starting the application.

## What Is Imported

| FlowJo | CytoFlow.rs |
|---|---|
| Sample list (`<SampleList>`) | Samples, with their FCS files |
| Groups (`<Groups>`, with their sample references and group gates) | Groups with template gate trees; "All Samples" stays the built-in group |
| Each sample's own gate tree | That sample's gate tree, with the same population id for the same population path in every sample. A sample's gate that differs from its group's is marked **tailored** |
| Rectangle and range gates, quadrant gates | Rectangle gates (bounds drawn to the edge of the axis become open bounds) |
| Polygon gates | Polygon gates, evaluated on FlowJo's display grid (`gateResolution`) like FlowJo does |
| Ellipse gates (`EllipsoidGate` with foci and edge points) | Ellipse gates |
| Boolean gates (And / Or / Not) | Boolean gates |
| Negated gates (`eventsInside="0"`) | Negated gates |
| Transforms: `linear` (with gain), `log`, `logicle`, `fasinh`, `biex` | linear, log, logicle, asinh, biex. Each sample keeps its own transforms (as per-sample overrides); the defaults come from the sample with the largest gate tree |
| `hyperlog` transform | Approximated by logicle with the same T, W, M, A |
| Compensation matrix (`<Matrices>`, `Comp-` prefixed channels) | A custom compensation matrix; gates on `Comp-` channels use compensated values |

Population counts stored by FlowJo are used only by the test suite to check the import (see [Validation](../validation.md)); CytoFlow.rs always recomputes the counts from the FCS data.

## What Is Not Imported

- **CurlyQuad gates** and any other gate type not listed above: the population and all of its sub-populations are skipped, with a warning
- **Several different compensation matrices**: only the first one is used (with a warning) and applied to every sample
- **Statistics nodes, layouts, tables, keywords edited in FlowJo, derived parameters, plugins** (FlowJo's layout editor and table editor documents are not read)
- **Sample-specific display settings** such as zoom, plot type or colours

## Agreement with FlowJo

The importer is tested by recomputing every population count recorded in public FlowJo workspaces and comparing it with FlowJo's own number:

| Gate type and axes | Agreement |
|---|---|
| Rectangles and quadrants (linear, logicle, asinh axes) | Exact |
| Rectangles and quadrants on FlowJo `biex` axes | Exact |
| Polygons on linear axes | Exact |
| Polygons on `biex` axes | Within 2 events (1–2 boundary events) |
| Polygons on logicle axes (and populations below them) | Within 0.1 % (within 0.2 % for populations of 10,000+ events) |
| Ellipses on linear axes | Exact |
| Ellipses on logicle axes | **2–3 % fewer events than FlowJo** (known limitation) |

!!! warning "Ellipses on logicle axes"
    FlowJo draws ellipses on a display grid whose exact mapping on logicle axes is not documented. With the geometry that reproduces FlowJo exactly on linear axes, ellipses on logicle axes count 2.0–3.1 % fewer events than FlowJo in the public test workspace. If you import such a gate, check the population visually and, if needed, adjust it (or redraw it as a polygon). Details are in [Validation](../validation.md#known-limitations).

## Tips

- Keep the `.wsp` file next to the FCS files (or one folder above them): the files are then found automatically even if the workspace was written on another computer
- After importing, open the **Batch** view on a group to check the gates of every sample at a glance
- Compare a few counts with FlowJo's before relying on an imported analysis, especially for ellipse gates
