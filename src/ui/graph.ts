// Centre column: graph window (toolbar, axis controls, plot canvases), batch and layout views.
import { api } from "../ipc";
import { composite, copyPng } from "../plot/export";
import type { ViewAxes } from "../plot/gates";
import { NORM_LABEL, renderOverlay, type HistNorm, type OverlayStyle, type Series } from "../plot/overlay";
import { type PlotData, type PlotKind, readTheme, renderPlot } from "../plot/render";
import { type Frame, makeFrame, setupCanvas } from "../plot/scale";
import { gatePath, isLoaded, on, sample, setTool, state, statFor, type Tool, emit } from "../state";
import { initAxes } from "./axes";
import { initBatch, refreshBatch } from "./batch";
import { openBooleanDialog } from "./dialogs";
import { clear, fmtInt, h } from "./dom";
import { drawOverlay, initGating } from "./gating";
import { initLayout, refreshLayout } from "./layout";
import {
  clearOverlay, fetchSeries, initOverlay, onOverlayChange, openOverlayPicker, overlay as ov, overlayActive, setOverlayOption,
} from "./overlay";
import { run, status } from "./status";

/** Centre-column views. `state.view` is typed graph|batch upstream; "layout" is added here. */
export type View = "graph" | "batch" | "layout";
export const currentView = (): View => state.view as View;
export function setView(v: View): void {
  (state as unknown as { view: View }).view = v;
  emit("view");
}

export interface GraphCtx {
  frame(): Frame;
  data(): PlotData | null;
  axes(): ViewAxes | null;
  overlay: HTMLCanvasElement;
}

let base: HTMLCanvasElement;
let overlay: HTMLCanvasElement;
let wrap: HTMLElement;
let titleEl: HTMLElement;
let countEl: HTMLElement;
let graphView: HTMLElement;
let batchView: HTMLElement;
let layoutView: HTMLElement;
let ovBar: HTMLElement;
let ovBtn: HTMLButtonElement;
let series: Series[] | null = null;
let frame: Frame = makeFrame(400, 400, 1);
let data: PlotData | null = null;
let message = "";
let seq = 0;
let pendingFetch = 0;
const toolButtons = new Map<Tool, HTMLButtonElement>();
const kindButtons = new Map<PlotKind, HTMLButtonElement>();
const viewButtons = new Map<string, HTMLButtonElement>();

const TOOLS: [Tool, string, string, string][] = [
  ["select", "↖", "Select / edit gates", "V"],
  ["rect", "▭", "Rectangle gate", "R"],
  ["polygon", "⬠", "Polygon gate (double-click or Enter to close)", "P"],
  ["ellipse", "⬭", "Ellipse gate (drag a bounding box)", "E"],
  ["range", "⟷", "Range gate (histograms)", "G"],
  ["quadrant", "✚", "Quadrant gates (click the centre)", "Q"],
];
const KINDS: [PlotKind, string][] = [["pseudo", "Pseudocolor"], ["density", "Density"], ["contour", "Contour"]];

export const viewAxes = (): ViewAxes | null => (state.x ? { x: state.x, y: state.hist ? null : state.y } : null);

export function initGraph(root: HTMLElement): void {
  const seg = (cls = "") => h("div", { class: `segmented ${cls}` });
  const tools = seg();
  for (const [t, icon, title, key] of TOOLS) {
    const b = h("button", { class: "seg-btn", text: icon, title: `${title} (${key})`, onclick: () => setTool(t) });
    toolButtons.set(t, b);
    tools.append(b);
  }
  const boolBtn = h("button", { class: "btn small", text: "Boolean…", title: "AND / OR / NOT of existing populations", onclick: () => openBooleanDialog() });
  const kinds = seg();
  for (const [k, label] of KINDS) {
    const b = h("button", { class: "seg-btn text", text: label, onclick: () => ((state.plotKind = k), emit("plotkind")) });
    kindButtons.set(k, b);
    kinds.append(b);
  }
  ovBtn = h("button", { class: "btn small", text: "Overlay…", title: "Overlay other populations and/or samples on this plot", onclick: () => void openOverlayPicker() });
  const copyBtn = h("button", { class: "btn small", text: "Copy", title: "Copy this plot as a PNG image", onclick: () => void copyPlot() });
  const views = seg();
  for (const [v, label] of [["graph", "Graph"], ["batch", "Batch"], ["layout", "Layout"]] as const) {
    const b = h("button", { class: "seg-btn text", text: label, onclick: () => setView(v) });
    viewButtons.set(v, b);
    views.append(b);
  }
  titleEl = h("div", { class: "graph-title" });
  countEl = h("div", { class: "graph-count" });
  const axesBar = h("div", { class: "axes-bar" });
  base = h("canvas", { class: "plot-base" });
  overlay = h("canvas", { class: "plot-overlay", tabindex: 0 });
  wrap = h("div", { class: "plot-wrap" }, base, overlay);
  ovBar = h("div", { class: "overlay-bar" });
  graphView = h("div", { class: "graph-view" }, axesBar, ovBar, wrap);
  batchView = h("div", { class: "batch-view" });
  layoutView = h("div", { class: "layout-view" });
  root.append(
    h("div", { class: "graph-head" }, tools, boolBtn, h("span", { class: "spacer" }), copyBtn, ovBtn, kinds, views),
    h("div", { class: "graph-sub" }, titleEl, h("span", { class: "spacer" }), countEl),
    graphView, batchView, layoutView);

  initAxes(axesBar);
  initGating({ frame: () => frame, data: () => data, axes: viewAxes, overlay });
  initBatch(batchView);
  initLayout(layoutView);
  initOverlay();
  onOverlayChange(() => (updateOverlayBar(), scheduleFetch()));

  new ResizeObserver(() => resize()).observe(wrap);
  on("workspace", scheduleFetch);
  on("selection", scheduleFetch);
  on("axes", scheduleFetch);
  on("plotkind", () => (updateButtons(), render(), refreshBatchIfVisible()));
  on("theme", () => (render(), refreshBatchIfVisible()));
  on("tool", updateButtons);
  on("stats", () => (updateTitle(), drawOverlay()));
  on("gatesel", drawOverlay);
  on("view", () => (updateButtons(), refreshBatchIfVisible(), currentView() === "layout" && refreshLayout(), resize()));
  updateButtons();
  updateOverlayBar();
}

function refreshBatchIfVisible(): void {
  if (state.view === "batch") refreshBatch();
}

function updateOverlayBar(): void {
  clear(ovBar);
  const active = overlayActive();
  ovBar.hidden = !active;
  ovBtn.classList.toggle("primary", active);
  ovBtn.textContent = active ? `Overlay (${ov.items.length + 1})…` : "Overlay…";
  if (!active) return;
  const seg = (opts: [string, string][], cur: string, set: (v: string) => void) =>
    h("div", { class: "segmented" }, ...opts.map(([v, label]) =>
      h("button", { class: `seg-btn text${v === cur ? " active" : ""}`, text: label, onclick: () => set(v) })));
  ovBar.append(
    h("span", { class: "muted", text: `Overlay of ${ov.items.length + 1} series` }),
    state.hist
      ? seg((["mode", "total", "count"] as HistNorm[]).map((n) => [n, NORM_LABEL[n]]), ov.norm, (v) => setOverlayOption({ norm: v as HistNorm }))
      : seg([["contour", "Contours"], ["dots", "Dots"]], ov.style, (v) => setOverlayOption({ style: v as OverlayStyle })),
    h("span", { class: "spacer" }),
    h("button", { class: "btn small", text: "Edit…", onclick: () => void openOverlayPicker() }),
    h("button", { class: "btn small", text: "Clear", onclick: () => clearOverlay() }));
}

async function copyPlot(): Promise<void> {
  if (!data) return status.error("Nothing to copy");
  await run(async () => {
    const s = sample();
    const name = `${s?.name.replace(/\.[^.]+$/, "") ?? "plot"}_${(gatePath(state.gateId).split(" / ").pop() ?? "").replace(/[^\w.-]+/g, "_")}.png`;
    const r = await copyPng(composite([base, overlay], readTheme().plotBg), name);
    if (r === "copied") status.ok("Plot copied to the clipboard");
    else if (r === "saved") status.ok(`Saved ${name}`);
  });
}

function updateButtons(): void {
  toolButtons.forEach((b, t) => {
    b.classList.toggle("active", state.tool === t);
    const histOnly = t === "range";
    const twoDOnly = t === "rect" || t === "polygon" || t === "ellipse" || t === "quadrant";
    b.disabled = (state.hist && twoDOnly) || (!state.hist && histOnly);
  });
  kindButtons.forEach((b, k) => {
    b.classList.toggle("active", state.plotKind === k);
    b.disabled = state.hist || overlayActive();
  });
  viewButtons.forEach((b, v) => b.classList.toggle("active", currentView() === v));
  graphView.hidden = currentView() !== "graph";
  batchView.hidden = currentView() !== "batch";
  layoutView.hidden = currentView() !== "layout";
  overlay.style.cursor = state.tool === "select" ? "default" : "crosshair";
}

function updateTitle(): void {
  const s = sample();
  titleEl.textContent = s ? `${s.name}  ›  ${gatePath(state.gateId)}` : "No sample selected";
  const st = statFor(state.gateId);
  countEl.textContent = st ? `${fmtInt(st.count)} events${state.gateId ? ` · ${((100 * st.count) / Math.max(1, st.parent_count)).toFixed(1)}% of parent` : ""}` : "";
}

function resize(): void {
  if (state.view !== "graph") return;
  const r = wrap.getBoundingClientRect();
  const size = Math.max(200, Math.floor(Math.min(r.width, r.height)));
  frame = makeFrame(size, size, window.devicePixelRatio || 1);
  setupCanvas(base, size, size);
  setupCanvas(overlay, size, size);
  render();
}

export function render(): void {
  const ctx = base.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(frame.dpr, 0, 0, frame.dpr, 0, 0);
  if (series && data) renderOverlay(ctx, frame, series, { norm: ov.norm, style: ov.style }, readTheme());
  else renderPlot(ctx, frame, data, { kind: state.plotKind }, readTheme(), message);
  drawOverlay();
}

function scheduleFetch(): void {
  updateTitle();
  updateButtons();
  updateOverlayBar();
  window.clearTimeout(pendingFetch);
  pendingFetch = window.setTimeout(fetchData, 16);
  refreshBatchIfVisible();
}

async function fetchData(): Promise<void> {
  const my = ++seq;
  const sid = state.sampleId;
  const x = state.x, y = state.y;
  const done = (d: PlotData | null, msg = "", ser: Series[] | null = null) => {
    if (my !== seq) return;
    data = d;
    series = ser;
    message = msg;
    render();
  };
  if (!sid) return done(null, state.ws?.samples.length ? "Select a sample" : "Add FCS files to begin (⌘O)");
  if (!isLoaded(sid)) return done(null, "Sample data not loaded (file missing)");
  if (!x || (!state.hist && !y)) return done(null, "Choose channels");
  try {
    if (overlayActive()) {
      const ser = await fetchSeries(x, state.hist ? null : y);
      return done(ser[0]?.data ?? null, "", ser);
    }
    if (state.hist) done({ kind: "hist", hist: await api.plotHist(sid, state.gateId, x, 256) });
    else done({ kind: "2d", plot: await api.plot2d(sid, state.gateId, x, y!, 256, 60000) });
  } catch (e) {
    if (my !== seq) return;
    status.error(e instanceof Error ? e.message : String(e));
    done(null, "Could not draw this plot");
  }
}
