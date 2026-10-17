// User-level actions shared by the toolbar, menus and keyboard shortcuts.
import { api } from "../ipc";
import { GATE_COLORS } from "../plot/colormap";
import { applyWorkspace, gate, isLoaded, selectPopulation, state } from "../state";
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

export const renameGate = (id: string) => run(async () => {
  const g = gate(id);
  if (!g) return;
  const name = await promptText("Rename population", "Name", g.name);
  if (name && name !== g.name) await applyWorkspace(await api.gateUpdate(id, { name }));
});

export const deleteGate = (id: string) => run(async () => {
  const g = gate(id);
  if (!g) return;
  const kids = state.ws?.gates.filter((x) => x.parent === id).length ?? 0;
  const msg = kids ? `Delete “${g.name}” and all of its sub-populations?` : `Delete “${g.name}”?`;
  if (!(await confirmBox("Delete gate", msg, "Delete", true))) return;
  if (state.gateId === id) selectPopulation(g.parent);
  await applyWorkspace(await api.gateRemove(id));
  status.info(`Deleted ${g.name}`);
});

export const toggleNegate = (id: string) => run(async () => {
  const g = gate(id);
  if (g) await applyWorkspace(await api.gateUpdate(id, { negate: !g.gate.negate }));
});

export const setGateColor = (id: string, color: string | null) => run(async () => {
  await applyWorkspace(await api.gateUpdate(id, { color }));
});

export function chooseColor(id: string, anchor: HTMLElement): void {
  const g = gate(id);
  const custom = h("input", { type: "color", value: g?.color ?? GATE_COLORS[0] });
  let close = () => {};
  custom.addEventListener("change", () => (close(), setGateColor(id, custom.value)));
  const grid = h("div", { class: "swatches" },
    ...GATE_COLORS.map((c) => h("button", { class: "swatch", style: `background:${c}`, title: c, onclick: () => (close(), setGateColor(id, c)) })),
    h("button", { class: "swatch none", title: "Automatic", text: "×", onclick: () => (close(), setGateColor(id, null)) }));
  close = popover(anchor, h("div", { class: "color-pop" }, grid, h("label", { class: "field-inline" }, "Custom ", custom)));
}

export async function showAbout(): Promise<void> {
  await alertBox("CytoFlow", "Open-source flow cytometry analysis. Gates are stored in display space; transforms remap them automatically.");
}
