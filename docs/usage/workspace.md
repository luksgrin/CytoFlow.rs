# Workspace and Samples

A **workspace** is everything you are working on: the list of samples, the sample groups, the gate tree of every sample, axis scales, compensation, layout pages and the last view of each sample. One workspace is open at a time.

## Samples

### Adding FCS files

Click **Add FCS…** in the toolbar (++cmd+o++ / ++ctrl+o++) and select one or more files (`.fcs`, `.FCS`, `.lmd`). Every sample appears in the **Samples** list with:

- its file name,
- the cytometer name (`$CYT` keyword) underneath, and the number of [tailored gates](#tailored-gates) if any,
- its event count on the right.

A new sample receives a copy of the gates of the **All Samples** group, so a gate tree you have already built is applied to new files straight away.

Click a sample (or use the ++up++ / ++down++ keys while the list has focus) to plot it. CytoFlow.rs remembers the axes and population last shown for each sample and restores them when you come back to it.

### Missing files

Workspaces store the path of each FCS file, not its data. If a file cannot be found when a workspace is opened, the sample stays in the list, greyed out with "file not found", and its plots and statistics are unavailable. When opening, CytoFlow.rs also looks for the file name next to the workspace file, so a folder containing both the workspace and its FCS files can be moved as a whole.

### Removing a sample

Right-click the sample and choose **Remove from workspace**. Its gates are removed with it; the FCS file on disk is not touched.

### Keywords

Right-click a sample and choose **Keywords…**, or click **Keywords…** in the right column, to see the full TEXT segment of the FCS file: every keyword (`$CYT`, `$DATE`, `$PnN`, `$PnS`, `$SPILLOVER`, custom keywords…) and its value. The filter box searches both names and values.

The stain name of each channel (`$PnS`) is shown next to the channel name in the axis menus, for example `CD4 (PE-A)`.

## Groups

Groups let you gate many samples at once.

- **All Samples** (◆) always exists and always contains every sample. It cannot be edited or deleted.
- Create your own group with **New group…** in the toolbar, the **+** next to **Groups**, or right-click a sample › **New group with this sample…**. Give it a name and tick its samples.
- Double-click a group (or right-click › **Edit samples…**) to change its members. Samples added to a group receive copies of the group's gates they do not have yet; samples removed from it keep theirs.
- Right-click › **Delete group** removes the group; its samples keep their gates.

### Gating scope

The coloured pill in the toolbar tells you where the gates you draw or edit go:

- **Gating: *group name*** (a group is selected): the gate is added to the group's *template* tree and copied into every member sample. Editing or deleting a gate changes it in every member (except members where it has been tailored, see below).
- **Gating: *sample name* only** (a sample is selected): the gate is added to, or changed in, that sample only.

When a group is selected, the **Populations** tree shows the group's template and the plot keeps showing the current sample if it is a member, or switches to the group's first member (the plotted sample is outlined in the sample list); samples outside the group are dimmed. Click a sample to leave the group and work on that sample alone.

## Population Tree

The **Populations** tree under the samples shows the gate hierarchy of the selected group or sample. "All events" is the root; each gate defines a child population of the population it was drawn in. Each row shows:

- the gate's colour,
- the population name (with "(NOT)" for a negated gate),
- a **T** badge if the gate is tailored for this sample,
- the event count and the percentage of the parent population, for the plotted sample.

Click a row to plot that population; the plot then shows only its events, and its child gates are drawn on top. Click the ▾ / ▸ arrow to collapse or expand a branch. ++up++ / ++down++ move through the tree when it has focus, and ++alt+left++ goes to the parent population from anywhere.

The buttons above the tree act on the selected population: **Aa** rename, **●** change colour, **¬** negate, **∩** new Boolean gate, **✕** delete. The same actions (and **Apply to group…**) are on the right-click menu. Deleting a gate also deletes all of its sub-populations, after confirmation.

## Tailored Gates

Every sample owns its own gate tree; a group's tree is a template that is copied into its members, with the same population ids in every copy.

When you edit a gate while a **sample** is selected (move it, reshape it, rename it, negate it), only that sample's copy changes and it becomes **tailored**: the row gets a **T** badge, the sample list shows "1 tailored", and later edits made through the group no longer overwrite that gate's shape in this sample (name and colour changes from the group still apply).

To push a tailored gate back to a group, right-click it in the sample's tree and choose **Apply to group…**. If the sample belongs to several groups you are asked which one; the group's template takes the gate and every member's copy is reset to it (and is no longer tailored).

Axis scales can be tailored too: changing a channel's scale while a sample is selected sets an override for that sample only; with a group selected it changes the default for all samples that have no override. See [The graph window](graph.md#axis-scales).

## Saving and Opening Workspaces

- **Save…** (++cmd+s++ / ++ctrl+s++) saves the workspace. The first save asks for a file name; later saves with the shortcut overwrite the same file. The toolbar button and ++shift+cmd+s++ / ++shift+ctrl+s++ always ask for a file name (Save As).
- **Open…** (++shift+cmd+o++ / ++shift+ctrl+o++) opens a `*.cfw.json` workspace and reloads its FCS files.
- **New** starts an empty workspace (after confirmation if samples are loaded).

The workspace name and "(unsaved)" until it has been saved are shown in the toolbar. The file is plain JSON (see [File formats](../formats.md#workspace-files-cfwjson)) and contains no event data.

FlowJo workspaces are opened with **Import FlowJo .wsp…**; see [Importing FlowJo workspaces](flowjo-import.md).
