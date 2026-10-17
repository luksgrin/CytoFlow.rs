// Left column: sample list and the population tree of the selected sample.
import { GATE_COLORS } from "../plot/colormap";
import { children, gate, isLoaded, on, sample, selectPopulation, selectSample, state, statFor } from "../state";
import type { GateNode } from "../types";
import { chooseColor, deleteGate, removeSample, renameGate, toggleNegate } from "./actions";
import { openBooleanDialog, openKeywords } from "./dialogs";
import { clear, fmtInt, fmtPct, h, pct } from "./dom";
import { showMenu } from "./modal";
import { run } from "./status";

let sampleList: HTMLElement;
let treeEl: HTMLElement;
let treeTitle: HTMLElement;
const collapsed = new Set<string>();

export function gateColor(g: GateNode): string {
  if (g.color) return g.color;
  const i = state.ws?.gates.indexOf(g) ?? 0;
  return GATE_COLORS[Math.max(0, i) % GATE_COLORS.length];
}

export function initWorkspacePanel(root: HTMLElement): void {
  sampleList = h("div", { class: "list", role: "listbox", tabindex: 0 });
  treeEl = h("div", { class: "tree", role: "tree", tabindex: 0 });
  treeTitle = h("span", { text: "Populations" });
  const btn = (label: string, title: string, fn: (id: string, el: HTMLElement) => void) => {
    const b: HTMLButtonElement = h("button", { class: "icon-btn", text: label, title, onclick: () => state.gateId && fn(state.gateId, b) });
    return b;
  };
  root.append(
    h("div", { class: "section-head" }, h("span", { text: "Samples" })),
    sampleList,
    h("div", { class: "section-head" }, treeTitle, h("span", { class: "spacer" }),
      btn("Aa", "Rename population", (id) => renameGate(id)),
      btn("●", "Change colour", (id, el) => chooseColor(id, el)),
      btn("¬", "Negate gate (NOT)", (id) => toggleNegate(id)),
      h("button", { class: "icon-btn", text: "∩", title: "Boolean gate…", onclick: () => openBooleanDialog() }),
      btn("✕", "Delete gate", (id) => deleteGate(id))),
    treeEl,
  );
  sampleList.addEventListener("keydown", (e) => {
    const ids = state.ws?.samples.map((s) => s.id) ?? [];
    const i = ids.indexOf(state.sampleId ?? "");
    if (e.key === "ArrowDown" && i < ids.length - 1) (e.preventDefault(), run(() => selectSample(ids[i + 1])));
    if (e.key === "ArrowUp" && i > 0) (e.preventDefault(), run(() => selectSample(ids[i - 1])));
  });
  treeEl.addEventListener("keydown", (e) => {
    const ids = flatIds();
    const i = ids.indexOf(state.gateId);
    if (e.key === "ArrowDown" && i < ids.length - 1) (e.preventDefault(), selectPopulation(ids[i + 1]));
    if (e.key === "ArrowUp" && i > 0) (e.preventDefault(), selectPopulation(ids[i - 1]));
  });
  on("workspace", renderAll);
  on("selection", renderAll);
  on("stats", renderTree);
  renderAll();
}

function renderAll(): void {
  renderSamples();
  renderTree();
}

function renderSamples(): void {
  clear(sampleList);
  const samples = state.ws?.samples ?? [];
  if (!samples.length) {
    sampleList.append(h("div", { class: "empty", text: "No samples. Use Add FCS… (⌘O)." }));
    return;
  }
  for (const s of samples) {
    const loaded = isLoaded(s.id);
    const cyt = s.keywords["$CYT"] ?? s.keywords["CYT"] ?? "";
    const row = h("div", {
      class: `list-row${s.id === state.sampleId ? " selected" : ""}${loaded ? "" : " missing"}`,
      role: "option", title: s.path,
      onclick: () => run(() => selectSample(s.id)),
      oncontextmenu: (e: Event) => {
        const me = e as MouseEvent;
        me.preventDefault();
        void run(() => selectSample(s.id));
        showMenu(me.clientX, me.clientY, [
          { label: "Keywords…", action: () => openKeywords(s.id), disabled: !loaded },
          { label: "Remove from workspace", action: () => removeSample(s.id), danger: true },
        ]);
      },
    },
      h("span", { class: "row-main" }, h("span", { class: "name", text: s.name }),
        h("span", { class: "sub", text: loaded ? cyt || "—" : "file not found" })),
      h("span", { class: "num", text: fmtInt(s.event_count) }));
    sampleList.append(row);
  }
}

function flatIds(): (string | null)[] {
  const out: (string | null)[] = [null];
  const walk = (p: string | null) => children(p).forEach((g) => (out.push(g.id), !collapsed.has(g.id) && walk(g.id)));
  walk(null);
  return out;
}

function nodeRow(g: GateNode | null, depth: number): HTMLElement {
  const id = g?.id ?? null;
  const st = statFor(id);
  const kids = children(id);
  const twisty = h("span", { class: "twisty", text: kids.length && g ? (collapsed.has(g.id) ? "▸" : "▾") : "" });
  if (g && kids.length) twisty.addEventListener("click", (e) => {
    e.stopPropagation();
    collapsed.has(g.id) ? collapsed.delete(g.id) : collapsed.add(g.id);
    renderTree();
  });
  const kind = g ? g.gate.shape.type : "root";
  const swatch = g ? h("span", { class: "swatch-dot", style: `background:${gateColor(g)}` }) : h("span", { class: "swatch-dot root" });
  const row = h("div", {
    class: `tree-row${state.gateId === id ? " selected" : ""}`, role: "treeitem",
    style: `padding-left:${8 + depth * 14}px`, title: g ? `${kind} gate${g.gate.negate ? " (negated)" : ""}` : "All events",
    onclick: () => selectPopulation(id),
    ondblclick: () => g && renameGate(g.id),
    oncontextmenu: (e: Event) => {
      const me = e as MouseEvent;
      me.preventDefault();
      selectPopulation(id);
      if (!g) return showMenu(me.clientX, me.clientY, [{ label: "Boolean gate…", action: () => openBooleanDialog() }]);
      showMenu(me.clientX, me.clientY, [
        { label: "Rename…", action: () => renameGate(g.id) },
        { label: "Change colour…", action: () => chooseColor(g.id, row) },
        { label: "Negate (NOT)", checked: g.gate.negate, action: () => toggleNegate(g.id) },
        { sep: true, label: "" },
        { label: "Boolean gate…", action: () => openBooleanDialog() },
        { sep: true, label: "" },
        { label: "Delete", danger: true, action: () => deleteGate(g.id) },
      ]);
    },
  },
    twisty, swatch,
    h("span", { class: "name", text: `${g ? g.name : "All events"}${g?.gate.negate ? " (NOT)" : ""}` }),
    h("span", { class: "num", text: st ? fmtInt(st.count) : "" }),
    h("span", { class: "pct", text: st && g ? `${fmtPct(pct(st.count, st.parent_count))}%` : "" }));
  return row;
}

function renderTree(): void {
  clear(treeEl);
  const s = sample();
  treeTitle.textContent = s ? `Populations — ${s.name}` : "Populations";
  if (!s) return;
  if (!isLoaded(s.id)) treeEl.append(h("div", { class: "empty", text: "Sample data not loaded; counts unavailable." }));
  const walk = (p: string | null, depth: number) => {
    for (const g of children(p)) {
      treeEl.append(nodeRow(g, depth));
      if (!collapsed.has(g.id)) walk(g.id, depth + 1);
    }
  };
  treeEl.append(nodeRow(null, 0));
  walk(null, 1);
  if (state.gateId && !gate(state.gateId)) selectPopulation(null);
  treeEl.querySelector(".tree-row.selected")?.scrollIntoView({ block: "nearest" });
}
