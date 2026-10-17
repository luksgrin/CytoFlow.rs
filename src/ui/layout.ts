// Layout editor: named printable pages of plot / histogram / statistics panels
// that re-render from live data, with PNG and SVG export. The pages are saved
// in the workspace (`layouts_set`, debounced); see layoutmodel.ts.
import "../layout.css";
import { api } from "../ipc";
import { savePng, saveSvg } from "../plot/export";
import { on, persistLater, setUiState, state, uiState, workspaceGeneration } from "../state";
import { clear, h } from "./dom";
import {
  CAPTION_H, clearPanelCache, emptyLayout, fetchPanel, fromStored, type Layout, loadDraft, MARGIN, type NamedLayout,
  newLayoutId, newPanelId, nextLayoutName, type PageSize, type Panel, type PanelData, pageDims, panelForCurrent,
  removeDraft, saveDraft, takeLegacyLayout, TITLE_H, toStored,
} from "./layoutmodel";
import { pagePng, pageSvg, panelCaption, panelTitle, renderPanelBody } from "./layoutrender";
import { confirmBox, promptText } from "./modal";
import { run, status } from "./status";

let root: HTMLElement;
let pageEl: HTMLElement;
let scroller: HTMLElement;
let sizeSel: HTMLSelectElement;
let orientSel: HTMLSelectElement;
let zoomSel: HTMLSelectElement;
let layoutSel: HTMLSelectElement;
let layouts: NamedLayout[] = [{ id: newLayoutId(), name: "Layout 1", data: emptyLayout() }];
let activeId = layouts[0].id;
let layout: Layout = layouts[0].data;
let gen = -1; // workspace generation the layouts were loaded for
let draftName: string | null = null; // workspace name a browser draft was written under
let selected: string | null = null;
let zoom = 0.75;
let seq = 0;
let refreshTimer = 0;
const data = new Map<string, PanelData>();
const views = new Map<string, { el: HTMLElement; canvas: HTMLCanvasElement; cap1: HTMLElement; cap2: HTMLElement }>();

const visible = () => !!root && !root.hidden;
const bodyH = (p: Panel) => Math.max(20, p.h - TITLE_H - CAPTION_H);

export function initLayout(el: HTMLElement): void {
  root = el;
  const b = (text: string, title: string, fn: () => unknown, cls = "btn small") => h("button", { class: cls, text, title, onclick: () => void fn() });
  const sel = (opts: [string, string][], onchange: (v: string) => void) => {
    const s = h("select", { class: "input select small" }, ...opts.map(([v, t]) => h("option", { value: v, text: t })));
    s.addEventListener("change", () => onchange(s.value));
    return s;
  };
  sizeSel = sel([["a4", "A4"], ["letter", "Letter"]], (v) => ((layout.size = v as PageSize), changed(true)));
  orientSel = sel([["portrait", "Portrait"], ["landscape", "Landscape"]], (v) => ((layout.landscape = v === "landscape"), changed(true)));
  zoomSel = sel([["0.5", "50%"], ["0.75", "75%"], ["1", "100%"], ["1.25", "125%"]], (v) => ((zoom = Number(v)), build()));
  zoomSel.value = String(zoom);
  pageEl = h("div", { class: "lp-page", onpointerdown: (e: Event) => e.target === pageEl && select(null) });
  scroller = h("div", { class: "lp-scroll" }, h("div", { class: "lp-sizer" }, pageEl));
  layoutSel = h("select", { class: "input select small", title: "Layouts of this workspace" });
  layoutSel.addEventListener("change", () => activate(layoutSel.value));
  root.append(
    h("div", { class: "lp-bar" },
      layoutSel,
      b("New", "Add an empty layout", newLayout),
      b("Rename…", "Rename this layout", renameLayout),
      b("Duplicate", "Copy this layout", duplicateLayout),
      b("Delete", "Delete this layout", deleteLayout),
      h("span", { class: "lp-sep" }),
      b("Add current graph", "Add the graph window's plot (sample, population, axes)", () => addCurrent()),
      b("Add for every sample", "One panel per sample with the current axes and population", () => addEverySample()),
      b("Add histogram", "Histogram of the current X channel", () => addCurrent(true)),
      b("Add stats table", "Population statistics of the current sample", () => addStats()),
      h("span", { class: "lp-sep" }), sizeSel, orientSel, zoomSel,
      h("span", { class: "spacer" }),
      b("Refresh", "Re-read all panels from the current data", () => refreshLayout(true)),
      b("Export PNG…", "Export the page as a PNG image (2×)", exportPng),
      b("Export SVG…", "Export the page as an SVG vector image", exportSvg),
      b("Clear", "Remove all panels", clearPage)),
    scroller);
  on("workspace", onWorkspace);
  onWorkspace();
}

function onWorkspace(): void {
  if (gen !== workspaceGeneration()) {
    gen = workspaceGeneration();
    loadFromWorkspace();
    selected = null;
    data.clear();
    updateSelector();
    if (visible()) build();
  }
  // Once the workspace has a file, the layouts live there: drop the browser draft.
  if (state.ws?.path && draftName !== null) {
    removeDraft(draftName);
    draftName = null;
  }
  clearPanelCache();
  if (visible()) scheduleRefresh();
}

/**
 * Take the layouts of a newly loaded workspace. A workspace without layouts
 * imports (once) the layout an earlier version kept in the browser store, or,
 * if it has never been saved, the browser draft.
 */
function loadFromWorkspace(): void {
  const ws = state.ws;
  const name = ws?.name ?? "";
  let list = fromStored(ws?.layouts);
  let push = false;
  draftName = null;
  if (!list.length && ws?.samples.length) {
    const legacy = takeLegacyLayout(name);
    if (legacy) {
      list = [{ id: newLayoutId(), name: "Layout 1", data: legacy }];
      push = true;
      status.info("Moved the layout kept in the browser into the workspace");
    } else if (!ws.path) {
      list = loadDraft(name);
      push = list.length > 0;
    }
  }
  if (!list.length) list = [{ id: newLayoutId(), name: "Layout 1", data: emptyLayout() }];
  layouts = list;
  const want = uiState().activeLayout;
  activeId = typeof want === "string" && list.some((l) => l.id === want) ? want : list[0].id;
  layout = active().data;
  if (push) persist();
}

const active = (): NamedLayout => layouts.find((l) => l.id === activeId) ?? layouts[0];

/** Save the layouts into the workspace (debounced) and, while it has no file, into the browser draft. */
function persist(): void {
  persistLater("layouts", async () => {
    const v = await api.layoutsSet(toStored(layouts));
    if (state.ws) state.ws.layouts = v.layouts;
  });
  const ws = state.ws;
  if (ws && !ws.path) {
    draftName = ws.name;
    saveDraft(ws.name, layouts);
  }
}

function updateSelector(): void {
  if (!layoutSel) return;
  clear(layoutSel);
  for (const l of layouts) layoutSel.append(h("option", { value: l.id, text: l.name }));
  layoutSel.value = activeId;
}

function activate(id: string): void {
  if (!layouts.some((l) => l.id === id)) return;
  activeId = id;
  layout = active().data;
  selected = null;
  data.clear();
  setUiState({ activeLayout: id });
  updateSelector();
  refreshLayout();
}

function addLayout(l: NamedLayout): void {
  layouts.push(l);
  persist();
  activate(l.id);
}

const newLayout = () => addLayout({ id: newLayoutId(), name: nextLayoutName(layouts), data: emptyLayout() });

const duplicateLayout = () => {
  const src = active();
  const copy = JSON.parse(JSON.stringify(src.data)) as Layout;
  copy.panels.forEach((p) => (p.id = newPanelId()));
  addLayout({ id: newLayoutId(), name: nextLayoutName(layouts, `${src.name} copy`), data: copy });
};

const renameLayout = () => run(async () => {
  const l = active();
  const name = (await promptText("Rename layout", "Name", l.name))?.trim();
  if (!name || name === l.name) return;
  l.name = name;
  persist();
  updateSelector();
});

const deleteLayout = () => run(async () => {
  const l = active();
  if (l.data.panels.length && !(await confirmBox("Delete layout", `Delete the layout “${l.name}”?`, "Delete", true))) return;
  layouts = layouts.filter((x) => x.id !== l.id);
  if (!layouts.length) layouts.push({ id: newLayoutId(), name: "Layout 1", data: emptyLayout() });
  persist();
  activate(layouts[0].id);
});

function scheduleRefresh(): void {
  window.clearTimeout(refreshTimer);
  refreshTimer = window.setTimeout(() => refreshLayout(), 60);
}

function changed(rebuild = false): void {
  persist();
  if (rebuild) build();
}

/** Rebuild the page DOM and re-render every panel (fetching data as needed). */
export function refreshLayout(force = false): void {
  if (!root) return;
  if (force) {
    clearPanelCache();
    data.clear();
  }
  build();
  void renderAll();
}

// ---------------------------------------------------------------- adding panels

function freeSpot(w: number, hgt: number): [number, number] {
  const [pw, ph] = pageDims(layout);
  const overlaps = (x: number, y: number) => layout.panels.some((p) => x < p.x + p.w && x + w > p.x && y < p.y + p.h && y + hgt > p.y);
  for (let y = MARGIN; y + hgt <= ph - MARGIN; y += 12)
    for (let x = MARGIN; x + w <= pw - MARGIN; x += 12) if (!overlaps(x, y)) return [x, y];
  return [MARGIN + (layout.panels.length % 8) * 14, MARGIN + (layout.panels.length % 8) * 14];
}

function add(p: Panel): void {
  [p.x, p.y] = freeSpot(p.w, p.h);
  layout.panels.push(p);
  selected = p.id;
  changed();
  refreshLayout();
}

function addCurrent(hist = state.hist): void {
  const p = panelForCurrent("plot", state.sampleId, hist);
  if (!p) return void status.error("Select a sample and axes first");
  add(p);
}

function addStats(): void {
  const p = panelForCurrent("stats");
  if (!p) return void status.error("Select a sample first");
  p.w = 320;
  p.h = TITLE_H + CAPTION_H + 15 * ((state.ws?.gates.length ?? 0) + 2) + 6;
  add(p);
}

function addEverySample(): void {
  const samples = state.ws?.samples ?? [];
  if (!samples.length || !state.x) return void status.error("Add samples and choose axes first");
  const [pw, ph] = pageDims(layout);
  const gap = 14;
  const cols = layout.landscape ? 4 : 3;
  const top = layout.panels.reduce((m, p) => Math.max(m, p.y + p.h + gap), MARGIN);
  const rows = Math.ceil(samples.length / cols);
  let cw = (pw - 2 * MARGIN - (cols - 1) * gap) / cols;
  const fitH = (ph - MARGIN - top - (rows - 1) * gap) / rows - TITLE_H - CAPTION_H;
  if (fitH < cw) cw = Math.max(110, fitH);
  samples.forEach((s, i) => {
    const p = panelForCurrent("plot", s.id);
    if (!p) return;
    p.w = Math.floor(cw);
    p.h = Math.floor(cw + TITLE_H + CAPTION_H);
    p.x = MARGIN + (i % cols) * (cw + gap);
    p.y = Math.min(ph - p.h - 4, top + Math.floor(i / cols) * (p.h + gap));
    p.title = s.name.replace(/\.[^.]+$/, "");
    layout.panels.push(p);
  });
  changed();
  refreshLayout();
}

const clearPage = () => run(async () => {
  if (!layout.panels.length || !(await confirmBox("Clear page", "Remove all panels from the layout?", "Clear", true))) return;
  layout.panels = [];
  changed(true);
});

// ---------------------------------------------------------------- DOM

function select(id: string | null): void {
  selected = id;
  views.forEach((v, k) => v.el.classList.toggle("selected", k === id));
}

export function deleteSelectedPanel(): boolean {
  if (!visible() || !selected) return false;
  layout.panels = layout.panels.filter((p) => p.id !== selected);
  selected = null;
  changed(true);
  return true;
}

function duplicate(p: Panel): void {
  add({ ...p, id: newPanelId() });
}

function build(): void {
  if (!root) return;
  const [pw, ph] = pageDims(layout);
  sizeSel.value = layout.size;
  orientSel.value = layout.landscape ? "landscape" : "portrait";
  pageEl.style.width = `${pw}px`;
  pageEl.style.height = `${ph}px`;
  pageEl.style.transform = `scale(${zoom})`;
  const sizer = pageEl.parentElement as HTMLElement;
  sizer.style.width = `${pw * zoom}px`;
  sizer.style.height = `${ph * zoom}px`;
  clear(pageEl);
  views.clear();
  if (!layout.panels.length)
    pageEl.append(h("div", { class: "lp-empty", text: "Empty page — use “Add current graph” or “Add for every sample” to place plots." }));
  for (const p of layout.panels) pageEl.append(panelEl(p));
  for (const p of layout.panels) paint(p);
}

function panelEl(p: Panel): HTMLElement {
  const title = h("input", { class: "lp-title", value: p.title, placeholder: panelTitle({ ...p, title: "" }), spellcheck: false, title: "Panel title" });
  title.addEventListener("change", () => ((p.title = title.value), changed()));
  const canvas = h("canvas", { class: "lp-canvas" });
  const cap1 = h("div", { class: "lp-cap1" }), cap2 = h("div", { class: "lp-cap2" });
  const tools = h("div", { class: "lp-tools" },
    h("button", { class: "icon-btn", text: "⧉", title: "Duplicate panel", onclick: () => duplicate(p) }),
    h("button", { class: "icon-btn", text: "✕", title: "Remove panel", onclick: () => (select(p.id), deleteSelectedPanel()) }));
  const grip = h("div", { class: "lp-resize", title: "Resize" });
  const el = h("div", { class: `lp-panel${p.id === selected ? " selected" : ""}` }, h("div", { class: "lp-head" }, title, tools), canvas, cap1, cap2, grip);
  place(el, p);
  el.addEventListener("pointerdown", (e) => startDrag(e, p, el, e.target === grip ? "resize" : "move"));
  views.set(p.id, { el, canvas, cap1, cap2 });
  return el;
}

function place(el: HTMLElement, p: Panel): void {
  el.style.left = `${p.x}px`;
  el.style.top = `${p.y}px`;
  el.style.width = `${p.w}px`;
  el.style.height = `${p.h}px`;
}

function startDrag(e: PointerEvent, p: Panel, el: HTMLElement, mode: "move" | "resize"): void {
  const t = e.target as HTMLElement;
  select(p.id);
  if (e.button !== 0 || t.tagName === "INPUT" || t.tagName === "BUTTON") return;
  e.preventDefault();
  const [pw, ph] = pageDims(layout);
  const sx = e.clientX, sy = e.clientY, ox = p.x, oy = p.y, ow = p.w, oh = p.h;
  el.setPointerCapture(e.pointerId);
  const snap = (v: number) => Math.round(v / 2) * 2;
  const move = (ev: PointerEvent) => {
    const dx = (ev.clientX - sx) / zoom, dy = (ev.clientY - sy) / zoom;
    if (mode === "move") {
      p.x = snap(Math.min(Math.max(0, ox + dx), pw - p.w));
      p.y = snap(Math.min(Math.max(0, oy + dy), ph - p.h));
    } else {
      p.w = snap(Math.min(Math.max(100, ow + dx), pw - p.x));
      p.h = snap(Math.min(Math.max(TITLE_H + CAPTION_H + 60, oh + dy), ph - p.y));
    }
    place(el, p);
  };
  const up = () => {
    el.removeEventListener("pointermove", move);
    el.removeEventListener("pointerup", up);
    el.removeEventListener("pointercancel", up);
    if (p.x === ox && p.y === oy && p.w === ow && p.h === oh) return;
    changed();
    if (mode === "resize") void renderOne(p);
  };
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", up);
  el.addEventListener("pointercancel", up);
}

// ---------------------------------------------------------------- rendering

function paint(p: Panel): void {
  const v = views.get(p.id);
  if (!v) return;
  const d = data.get(p.id) ?? null;
  const dpr = (window.devicePixelRatio || 1) * Math.max(1, zoom);
  const bh = bodyH(p);
  v.canvas.width = Math.max(1, Math.round(p.w * dpr));
  v.canvas.height = Math.max(1, Math.round(bh * dpr));
  v.canvas.style.width = `${p.w}px`;
  v.canvas.style.height = `${bh}px`;
  const ctx = v.canvas.getContext("2d");
  if (ctx) {
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = "#fff";
    ctx.fillRect(0, 0, p.w, bh);
    renderPanelBody(ctx, p, d, dpr);
  }
  const [c1, c2] = panelCaption(p, d);
  v.cap1.textContent = c1;
  v.cap2.textContent = c2;
  (v.el.querySelector(".lp-title") as HTMLInputElement | null)?.setAttribute("placeholder", panelTitle({ ...p, title: "" }));
}

async function renderOne(p: Panel): Promise<void> {
  try {
    data.set(p.id, await fetchPanel(p));
  } catch (e) {
    data.set(p.id, { plot: null, stats: null, message: "Error" });
    status.error(e instanceof Error ? e.message : String(e));
  }
  paint(p);
}

async function renderAll(): Promise<void> {
  const my = ++seq;
  for (const p of [...layout.panels]) {
    if (my !== seq) return;
    await renderOne(p);
  }
}

/** Fetch data for every panel (used by export so nothing renders as "Loading…"). */
async function ensureData(): Promise<void> {
  for (const p of layout.panels) if (!data.get(p.id)?.plot && !data.get(p.id)?.stats) data.set(p.id, await fetchPanel(p));
}

// ---------------------------------------------------------------- export

const fileBase = () => `${(state.ws?.name || "layout").replace(/[^\w.-]+/g, "_")}_layout`;

const exportPng = () => run(async () => {
  if (!layout.panels.length) return status.error("The page is empty");
  await ensureData();
  const path = await savePng(pagePng(layout, (p) => data.get(p.id) ?? null, 2), `${fileBase()}.png`);
  if (path) status.ok(`Exported ${path}`);
});

const exportSvg = () => run(async () => {
  if (!layout.panels.length) return status.error("The page is empty");
  await ensureData();
  const path = await saveSvg(pageSvg(layout, (p) => data.get(p.id) ?? null), `${fileBase()}.svg`);
  if (path) status.ok(`Exported ${path}`);
});

/** For tests / debugging: the current layout model. */
export const currentLayout = (): Layout => layout;
/** For tests / debugging: every layout of the workspace and the active one. */
export const currentLayouts = (): { active: string; layouts: NamedLayout[] } => ({ active: activeId, layouts });
