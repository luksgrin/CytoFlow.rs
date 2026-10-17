// Left column: groups, the sample list and the population tree of the
// selected group (its template) or sample (its own tree, tailored nodes marked).
import { GATE_COLORS } from "../plot/colormap";
import {
  children, currentGates, gate, group, groupsOf, isLoaded, on, sample, selectGroup, selectPopulation, selectSample,
  state, statFor,
} from "../state";
import { ALL_SAMPLES, type GateNode } from "../types";
import { applyToGroup, chooseColor, deleteGate, deleteGroup, removeSample, renameGate, toggleNegate } from "./actions";
import { openBooleanDialog, openGroupDialog, openKeywords } from "./dialogs";
import { clear, fmtInt, fmtPct, h, pct } from "./dom";
import { showMenu } from "./modal";
import { run } from "./status";

let groupList: HTMLElement;
let sampleList: HTMLElement;
let treeEl: HTMLElement;
let treeTitle: HTMLElement;
const collapsed = new Set<string>();

export function gateColor(g: GateNode): string {
  if (g.color) return g.color;
  const i = currentGates().findIndex((x) => x.id === g.id);
  return GATE_COLORS[Math.max(0, i) % GATE_COLORS.length];
}

export function initWorkspacePanel(root: HTMLElement): void {
  groupList = h("div", { class: "list groups", role: "listbox", tabindex: 0 });
  sampleList = h("div", { class: "list", role: "listbox", tabindex: 0 });
  treeEl = h("div", { class: "tree", role: "tree", tabindex: 0 });
  treeTitle = h("span", { text: "Populations" });
  const btn = (label: string, title: string, fn: (id: string, el: HTMLElement) => void) => {
    const b: HTMLButtonElement = h("button", { class: "icon-btn", text: label, title, onclick: () => state.gateId && fn(state.gateId, b) });
    return b;
  };
  root.append(
    h("div", { class: "section-head" }, h("span", { text: "Groups" }), h("span", { class: "spacer" }),
      h("button", { class: "icon-btn", text: "+", title: "New group…", onclick: () => openGroupDialog() })),
    groupList,
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
  groupList.addEventListener("keydown", (e) => {
    const ids = state.ws?.groups.map((g) => g.id) ?? [];
    const i = ids.indexOf(state.groupId ?? "");
    if (e.key === "ArrowDown" && i >= 0 && i < ids.length - 1) (e.preventDefault(), run(() => selectGroup(ids[i + 1])));
    if (e.key === "ArrowUp" && i > 0) (e.preventDefault(), run(() => selectGroup(ids[i - 1])));
  });
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
  renderGroups();
  renderSamples();
  renderTree();
}

function renderGroups(): void {
  clear(groupList);
  for (const g of state.ws?.groups ?? []) {
    const builtin = g.id === ALL_SAMPLES;
    const row = h("div", {
      class: `list-row group-row${g.id === state.groupId ? " selected" : ""}${builtin ? " builtin" : ""}`,
      role: "option",
      title: `${g.name}: ${g.sample_ids.length} sample(s), ${g.gates.length} template gate(s). Gates drawn while a group is selected apply to all of its samples.`,
      onclick: () => run(() => selectGroup(g.id)),
      ondblclick: () => !builtin && openGroupDialog(g.id),
      oncontextmenu: (e: Event) => {
        const me = e as MouseEvent;
        me.preventDefault();
        void run(() => selectGroup(g.id));
        showMenu(me.clientX, me.clientY, [
          { label: "Edit samples…", action: () => openGroupDialog(g.id), disabled: builtin },
          { label: "New group…", action: () => openGroupDialog() },
          { sep: true, label: "" },
          { label: "Delete group", action: () => deleteGroup(g.id), danger: true, disabled: builtin },
        ]);
      },
    },
      h("span", { class: "group-icon", text: builtin ? "◆" : "◇" }),
      h("span", { class: "row-main" }, h("span", { class: "name", text: g.name })),
      h("span", { class: "num", text: `${g.sample_ids.length}` }));
    groupList.append(row);
  }
}

function renderSamples(): void {
  clear(sampleList);
  const samples = state.ws?.samples ?? [];
  if (!samples.length) {
    sampleList.append(h("div", { class: "empty", text: "No samples. Use Add FCS… (⌘O)." }));
    return;
  }
  const grp = group();
  for (const s of samples) {
    const loaded = isLoaded(s.id);
    const cyt = s.keywords["$CYT"] ?? s.keywords["CYT"] ?? "";
    const selected = !grp && s.id === state.sampleId;
    const member = !!grp && grp.sample_ids.includes(s.id);
    const plotted = !!grp && s.id === state.sampleId;
    const nTailored = (state.ws?.sample_gates?.[s.id] ?? []).filter((g) => g.tailored).length;
    const row = h("div", {
      class: `list-row${selected ? " selected" : ""}${loaded ? "" : " missing"}${grp && !member ? " outside" : ""}${plotted ? " plotted" : ""}`,
      role: "option", title: `${s.path}${grp ? (member ? `\nIn group ${grp.name}` : `\nNot in group ${grp.name}`) : ""}`,
      onclick: () => run(() => selectSample(s.id)),
      oncontextmenu: (e: Event) => {
        const me = e as MouseEvent;
        me.preventDefault();
        void run(() => selectSample(s.id));
        showMenu(me.clientX, me.clientY, [
          { label: "Keywords…", action: () => openKeywords(s.id), disabled: !loaded },
          { label: "New group with this sample…", action: () => openGroupDialog() },
          { sep: true, label: "" },
          { label: "Remove from workspace", action: () => removeSample(s.id), danger: true },
        ]);
      },
    },
      h("span", { class: "row-main" }, h("span", { class: "name", text: s.name }),
        h("span", { class: "sub", text: loaded ? `${cyt || "—"}${nTailored ? ` · ${nTailored} tailored` : ""}` : "file not found" })),
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
  const tailored = !!g?.tailored && !state.groupId;
  const swatch = g ? h("span", { class: "swatch-dot", style: `background:${gateColor(g)}` }) : h("span", { class: "swatch-dot root" });
  const sid = state.sampleId;
  const inSampleScope = !state.groupId && !!sid;
  const row = h("div", {
    class: `tree-row${state.gateId === id ? " selected" : ""}${tailored ? " tailored" : ""}`, role: "treeitem",
    style: `padding-left:${8 + depth * 14}px`,
    title: g ? `${kind} gate${g.gate.negate ? " (negated)" : ""}${tailored ? " — tailored for this sample" : ""}` : "All events",
    onclick: () => selectPopulation(id),
    ondblclick: () => g && renameGate(g.id),
    oncontextmenu: (e: Event) => {
      const me = e as MouseEvent;
      me.preventDefault();
      selectPopulation(id);
      if (!g) return showMenu(me.clientX, me.clientY, [{ label: "Boolean gate…", action: () => openBooleanDialog() }]);
      const canApply = inSampleScope && groupsOf(sid).length > 0;
      showMenu(me.clientX, me.clientY, [
        { label: "Rename…", action: () => renameGate(g.id) },
        { label: "Change colour…", action: () => chooseColor(g.id, row) },
        { label: "Negate (NOT)", checked: g.gate.negate, action: () => toggleNegate(g.id) },
        ...(inSampleScope ? [
          { sep: true, label: "" },
          { label: tailored ? "Apply to group… (tailored)" : "Apply to group…", action: () => applyToGroup(g.id, sid), disabled: !canApply },
        ] : []),
        { sep: true, label: "" },
        { label: "Boolean gate…", action: () => openBooleanDialog() },
        { sep: true, label: "" },
        { label: inSampleScope ? "Delete from this sample" : "Delete", danger: true, action: () => deleteGate(g.id) },
      ]);
    },
  },
    twisty, swatch,
    h("span", { class: "name", text: `${g ? g.name : "All events"}${g?.gate.negate ? " (NOT)" : ""}` }),
    tailored ? h("span", { class: "badge tailored-badge", text: "T", title: "Tailored: this sample's gate differs from its group" }) : "",
    h("span", { class: "num", text: st ? fmtInt(st.count) : "" }),
    h("span", { class: "pct", text: st && g ? `${fmtPct(pct(st.count, st.parent_count))}%` : "" }));
  return row;
}

function renderTree(): void {
  clear(treeEl);
  const s = sample();
  const grp = group();
  treeTitle.textContent = grp ? `Populations — ${grp.name} (group)` : s ? `Populations — ${s.name}` : "Populations";
  treeTitle.title = grp
    ? `Template gates of ${grp.name}; edits apply to all ${grp.sample_ids.length} sample(s)${s ? `. Counts: ${s.name}` : ""}`
    : s ? `Gates of ${s.name}; edits apply to this sample only (tailored)` : "";
  if (!s && !grp) return;
  if (grp && !grp.sample_ids.length) treeEl.append(h("div", { class: "empty", text: "This group has no samples. Right-click › Edit samples…" }));
  else if (s && !isLoaded(s.id)) treeEl.append(h("div", { class: "empty", text: "Sample data not loaded; counts unavailable." }));
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
