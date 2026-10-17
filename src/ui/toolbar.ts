// Top toolbar.
import { isTauri } from "../ipc";
import { on, state } from "../state";
import { exportPopulation, exportStats, importWsp, newWorkspace, openFcs, openWorkspace, saveWorkspace } from "./actions";
import { openCompensation } from "./compensation";
import { h } from "./dom";
import { cycleTheme, themePref } from "./theme";

export function initToolbar(root: HTMLElement): void {
  const b = (text: string, title: string, fn: () => unknown, cls = "") => h("button", { class: `tb-btn ${cls}`, text, title, onclick: () => void fn() });
  const nameEl = h("span", { class: "ws-name" });
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
    h("div", { class: "tb-group" }, b("Compensation…", "Compensation matrix", openCompensation)),
    h("span", { class: "spacer" }),
    nameEl,
    themeBtn,
  );
  const upd = () => {
    const ws = state.ws;
    nameEl.textContent = ws ? `${ws.name || "Untitled"}${ws.path ? "" : " (unsaved)"}` : "";
    nameEl.title = ws?.path ?? "";
    document.title = ws?.name ? `${ws.name} — CytoFlow${isTauri ? "" : " (mock)"}` : "CytoFlow";
  };
  on("workspace", upd);
  upd();
}
