// Top toolbar.
import { isTauri } from "../ipc";
import { group, on, sample, state } from "../state";
import { exportPopulation, exportStats, importWsp, newWorkspace, openFcs, openWorkspace, saveWorkspace } from "./actions";
import { openCompensation } from "./compensation";
import { openGroupDialog } from "./dialogs";
import { h } from "./dom";
import { cycleTheme, themePref } from "./theme";

export function initToolbar(root: HTMLElement): void {
  const b = (text: string, title: string, fn: () => unknown, cls = "") => h("button", { class: `tb-btn ${cls}`, text, title, onclick: () => void fn() });
  const nameEl = h("span", { class: "ws-name" });
  // Where new / edited gates go: the selected group, or the selected sample only.
  const scopeEl = h("span", { class: "scope-pill" });
  const updScope = () => {
    const g = group();
    const s = sample();
    scopeEl.className = `scope-pill ${g ? "group" : "sample"}`;
    scopeEl.textContent = g ? `Gating: ${g.name}` : s ? `Gating: ${s.name} only` : "";
    scopeEl.title = g
      ? `Gates you draw or edit apply to the group “${g.name}” (${g.sample_ids.length} samples)`
      : s ? "Gates you draw or edit apply to this sample only (tailored)" : "";
  };
  on("selection", updScope);
  on("workspace", updScope);
  const themeBtn = b("", "Theme: system / light / dark", () => updateTheme(cycleTheme()));
  const updateTheme = (p = themePref()) => (themeBtn.textContent = p === "system" ? "◐ Auto" : p === "light" ? "☀ Light" : "☾ Dark");
  updateTheme();
  root.append(
    h("span", { class: "brand", text: "CytoFlow" }),
    h("div", { class: "tb-group" },
      b("Add FCS…", "Add FCS files (⌘O)", openFcs),
      b("New", "New workspace", newWorkspace),
      b("Open…", "Open workspace (⇧⌘O)", openWorkspace),
      b("Save…", "Save workspace (⌘S)", () => saveWorkspace(true))),
    h("div", { class: "tb-group" }, b("Import FlowJo .wsp…", "Import a FlowJo workspace", importWsp)),
    h("div", { class: "tb-group" },
      b("Export population…", "Export events of the selected population as CSV", exportPopulation),
      b("Export stats…", "Export the statistics table as CSV", exportStats)),
    h("div", { class: "tb-group" },
      b("Compensation…", "Compensation matrix", openCompensation),
      b("New group…", "Create a sample group; gates drawn on a selected group apply to all of its samples", () => openGroupDialog())),
    h("span", { class: "spacer" }),
    scopeEl,
    nameEl,
    themeBtn,
  );
  const upd = () => {
    const ws = state.ws;
    // "(unsaved)": the workspace has no file yet, so layouts / UI state exist only in memory (and a browser draft).
    const label = ws ? `${ws.name || "Untitled"}${ws.path ? "" : " (unsaved)"}` : "";
    nameEl.textContent = label;
    nameEl.title = ws?.path ?? "Not saved to a file yet";
    document.title = ws ? `${label} — CytoFlow${isTauri ? "" : " (mock)"}` : "CytoFlow";
  };
  on("workspace", upd);
  upd();
}
