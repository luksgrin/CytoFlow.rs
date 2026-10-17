// User-level actions shared by the toolbar, menus and keyboard shortcuts.
import { api } from "../ipc";
import { GATE_COLORS } from "../plot/colormap";
import {
  applyWorkspace, currentScope, gate, groupById, groupsOf, isLoaded, sampleGate, sampleGates, selectPopulation, state,
} from "../state";
import { ALL_SAMPLES, type GateNode, type GateScope } from "../types";
import { h } from "./dom";
import { dirname, pickDir, pickOpen, pickSave } from "./files";
import { alertBox, confirmBox, modal, popover, promptText } from "./modal";
import { run, status } from "./status";

const FCS = [{ name: "FCS files", extensions: ["fcs", "FCS", "lmd"] }];
const CFW = [{ name: "CytoFlow workspace", extensions: ["json"] }];
const WSP = [{ name: "FlowJo workspace", extensions: ["wsp", "xml"] }];
const CSV = [{ name: "CSV", extensions: ["csv"] }];

function reportErrors(errors: string[], what: string): void {
  if (errors.length) status.error(`${what}: ${errors.length} problem(s) — ${errors.join("; ")}`);
}

export const openFcs = () => run(async () => {
  const paths = await pickOpen("Add FCS files", FCS, true, "/mock/data/Donor4_PBMC.fcs");
  if (!paths?.length) return;
  status.info(`Loading ${paths.length} file(s)…`);
  const r = await api.sampleAdd(paths);
  await applyWorkspace(r.workspace);
  reportErrors(r.errors, "Open FCS");
  if (!r.errors.length) status.ok(`Added ${paths.length} sample(s)`);
});

export const openWorkspace = () => run(async () => {
  const p = await pickOpen("Open workspace", CFW, false, "/mock/demo.cfw.json");
  if (!p?.[0]) return;
  const r = await api.workspaceOpen(p[0]);
  state.gateId = null;
  await applyWorkspace(r.workspace);
  reportErrors(r.errors, "Open workspace");
  if (!r.errors.length) status.ok(`Opened ${r.workspace.name}`);
});

export const newWorkspace = () => run(async () => {
  if (state.ws?.samples.length && !(await confirmBox("New workspace", "Discard the current workspace?", "Discard", true))) return;
  state.gateId = null;
  await applyWorkspace(await api.workspaceNew());
});

export const saveWorkspace = (saveAs = false) => run(async () => {
  let path = state.ws?.path ?? null;
  if (saveAs || !path) path = await pickSave("Save workspace", CFW, path ?? `${state.ws?.name || "Untitled"}.cfw.json`);
  if (!path) return;
  if (!/\.json$/i.test(path)) path += ".cfw.json";
  await applyWorkspace(await api.workspaceSave(path));
  status.ok(`Saved ${path}`);
});

async function importWith(path: string, dirs: string[]): Promise<void> {
  const r = await api.workspaceImportWsp(path, dirs);
  state.gateId = null;
  await applyWorkspace(r.workspace);
  if (r.warnings.length) status.info(`Imported with ${r.warnings.length} warning(s): ${r.warnings.join("; ")}`);
  else status.ok(`Imported ${path}`);
  if (!r.missing.length) return;
  const list = h("ul", { class: "plain-list" }, ...r.missing.map((m) => h("li", {}, h("b", { text: m.name }), h("span", { class: "muted", text: ` ${m.path}` }))));
  const choice = await modal("Missing FCS files",
    h("div", {}, h("p", { text: `${r.missing.length} sample file(s) referenced by the workspace were not found.` }), list),
    [{ label: "Skip", value: "skip" }, { label: "Locate folder…", value: "locate", primary: true }]);
  if (choice !== "locate") return;
  const dir = await pickDir("Folder containing the FCS files");
  if (dir) await importWith(path, [...dirs, dir]);
}

export const importWsp = () => run(async () => {
  const p = await pickOpen("Import FlowJo workspace", WSP, false, "/mock/legacy.wsp");
  if (p?.[0]) await importWith(p[0], [dirname(p[0])]);
});

export const exportPopulation = () => run(async () => {
  const sid = state.sampleId;
  if (!sid || !isLoaded(sid)) return status.error("Select a loaded sample first");
  const comp = h("input", { type: "checkbox", checked: state.ws?.compensation.source !== "none" });
  const tr = h("input", { type: "checkbox" });
  const axesOnly = h("input", { type: "checkbox" });
  const body = h("div", { class: "form" },
    h("p", { class: "muted", text: `Population: ${gate(state.gateId)?.name ?? "All events"}` }),
    h("label", { class: "check-row" }, comp, " Compensated values"),
    h("label", { class: "check-row" }, tr, " Transformed (display-space) values"),
    h("label", { class: "check-row" }, axesOnly, " Only the channels on the current axes"));
  if ((await modal("Export population", body, [{ label: "Cancel", value: "c" }, { label: "Export…", value: "ok", primary: true }], { onEnter: "ok" })) !== "ok") return;
  const name = `${(gate(state.gateId)?.name ?? "All events").replace(/[^\w.-]+/g, "_")}.csv`;
  const path = await pickSave("Export population CSV", CSV, name);
  if (!path) return;
  const chans = axesOnly.checked ? [state.x?.channel, state.hist ? undefined : state.y?.channel].filter((c): c is string => !!c) : null;
  const n = await api.exportPopulationCsv(sid, state.gateId, path, chans, comp.checked, tr.checked);
  status.ok(`Wrote ${n.toLocaleString()} events to ${path}`);
});

export const exportStats = () => run(async () => {
  const path = await pickSave("Export statistics table", CSV, `${state.ws?.name || "stats"}_stats.csv`);
  if (!path) return;
  const n = await api.exportStatsCsv(path);
  status.ok(`Wrote ${n} rows to ${path}`);
});

export const removeSample = (id: string) => run(async () => {
  const s = state.ws?.samples.find((x) => x.id === id);
  if (!s || !(await confirmBox("Remove sample", `Remove ${s.name} from the workspace?`, "Remove", true))) return;
  await applyWorkspace(await api.sampleRemove(id));
});

// ---------------------------------------------------------------- gates

// Every gate helper takes an optional `scope` (default: `currentScope()`, i.e.
// the selected group, or the selected sample, which then becomes tailored).

/** Human-readable target of a scope, for status messages. */
function scopeLabel(scope: GateScope): string {
  if ("group" in scope) return `group ${groupById(scope.group)?.name ?? scope.group}`;
  return state.ws?.samples.find((s) => s.id === scope.sample)?.name ?? scope.sample;
}

/** The gate as seen in a scope's tree. */
function gateIn(id: string, scope: GateScope): GateNode | null {
  if ("group" in scope) return groupById(scope.group)?.gates.find((g) => g.id === id) ?? null;
  return sampleGate(id, scope.sample);
}

export const renameGate = (id: string, scope: GateScope = currentScope()) => run(async () => {
  const g = gateIn(id, scope) ?? gate(id);
  if (!g) return;
  const name = await promptText("Rename population", "Name", g.name);
  if (name && name !== g.name) await applyWorkspace(await api.gateUpdate(id, { name }, scope));
});

export const deleteGate = (id: string, scope: GateScope = currentScope()) => run(async () => {
  const g = gateIn(id, scope) ?? gate(id);
  if (!g) return;
  const tree = "group" in scope ? groupById(scope.group)?.gates ?? [] : sampleGates(scope.sample);
  const kids = tree.filter((x) => x.parent === id).length;
  const where = "group" in scope ? ` from ${scopeLabel(scope)} and all of its samples` : ` from ${scopeLabel(scope)} only`;
  const msg = kids ? `Delete “${g.name}” and all of its sub-populations${where}?` : `Delete “${g.name}”${where}?`;
  if (!(await confirmBox("Delete gate", msg, "Delete", true))) return;
  if (state.gateId === id) selectPopulation(g.parent);
  await applyWorkspace(await api.gateRemove(id, scope));
  status.info(`Deleted ${g.name}`);
});

export const toggleNegate = (id: string, scope: GateScope = currentScope()) => run(async () => {
  const g = gateIn(id, scope) ?? gate(id);
  if (g) await applyWorkspace(await api.gateUpdate(id, { negate: !g.gate.negate }, scope));
});

export const setGateColor = (id: string, color: string | null, scope: GateScope = currentScope()) => run(async () => {
  await applyWorkspace(await api.gateUpdate(id, { color }, scope));
});

export function chooseColor(id: string, anchor: HTMLElement, scope: GateScope = currentScope()): void {
  const g = gateIn(id, scope) ?? gate(id);
  const custom = h("input", { type: "color", value: g?.color ?? GATE_COLORS[0] });
  let close = () => {};
  custom.addEventListener("change", () => (close(), setGateColor(id, custom.value, scope)));
  const grid = h("div", { class: "swatches" },
    ...GATE_COLORS.map((c) => h("button", { class: "swatch", style: `background:${c}`, title: c, onclick: () => (close(), setGateColor(id, c, scope)) })),
    h("button", { class: "swatch none", title: "Automatic", text: "×", onclick: () => (close(), setGateColor(id, null, scope)) }));
  close = popover(anchor, h("div", { class: "color-pop" }, grid, h("label", { class: "field-inline" }, "Custom ", custom)));
}

/**
 * "Apply to group": push a sample's (tailored) gate to one of the sample's
 * groups; every member's copy is reset to it.
 */
export const applyToGroup = (gateId: string, sampleId: string | null = state.sampleId, groupId?: string) => run(async () => {
  if (!sampleId) return;
  const g = sampleGate(gateId, sampleId);
  if (!g) return status.error("This population is not in the selected sample");
  const groups = groupsOf(sampleId);
  let target = groupId ?? null;
  if (!target) {
    // Prefer a group whose template already has this gate (or its parent).
    const fits = groups.filter((x) => x.gates.some((t) => t.id === gateId || t.id === g.parent) || !g.parent);
    if (!fits.length) return status.error(`No group of this sample contains the parent population of “${g.name}”`);
    if (fits.length === 1) target = fits[0].id;
    else {
      const sel = h("select", { class: "input" }, ...fits.map((x) => h("option", { value: x.id, text: `${x.name} (${x.sample_ids.length} samples)` })));
      const pre = fits.find((x) => x.id !== ALL_SAMPLES && x.gates.some((t) => t.id === gateId)) ?? fits.find((x) => x.gates.some((t) => t.id === gateId));
      if (pre) sel.value = pre.id;
      const body = h("div", { class: "form" },
        h("p", { text: `Copy “${g.name}” from this sample to a group. Every sample of the group will use this gate.` }),
        h("label", { class: "field" }, h("span", { text: "Group" }), sel));
      if ((await modal("Apply to group", body, [{ label: "Cancel", value: "c" }, { label: "Apply", value: "ok", primary: true }], { onEnter: "ok" })) !== "ok") return;
      target = sel.value;
    }
  }
  await applyWorkspace(await api.gateCopyToGroup(gateId, sampleId, target));
  status.ok(`Applied “${g.name}” to ${scopeLabel({ group: target })}`);
});

// ---------------------------------------------------------------- groups

export const deleteGroup = (groupId: string) => run(async () => {
  const g = groupById(groupId);
  if (!g || g.id === ALL_SAMPLES) return;
  if (!(await confirmBox("Delete group", `Delete the group “${g.name}”? Its samples keep their gates.`, "Delete", true))) return;
  await applyWorkspace(await api.groupRemove(groupId));
  status.info(`Deleted group ${g.name}`);
});

export async function showAbout(): Promise<void> {
  await alertBox("CytoFlow", "Open-source flow cytometry analysis. Gates are stored in display space; transforms remap them automatically.");
}
