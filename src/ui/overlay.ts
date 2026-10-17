// Graph-window overlays: extra populations and/or samples drawn over the current plot.
// Data comes from plot_hist / plot_2d, called once per overlay item and cached.
// The selection (items, normalisation, style) is saved in the workspace's
// `ui_state.overlay` and restored when the workspace is reopened.
import { api } from "../ipc";
import { GATE_COLORS } from "../plot/colormap";
import type { HistNorm, OverlayStyle, Series } from "../plot/overlay";
import type { PlotData } from "../plot/render";
import { gate, gatePathIn, isLoaded, on, sampleGates, setUiState, state, uiState, workspaceGeneration } from "../state";
import type { GateDim } from "../types";
import { h } from "./dom";
import { modal } from "./modal";
import { status } from "./status";

export interface OverlayItem { sampleId: string; gateId: string | null }

export const overlay = {
  items: [] as OverlayItem[],
  norm: "mode" as HistNorm,
  style: "contour" as OverlayStyle,
};

const MAX_ITEMS = 11;
const listeners = new Set<() => void>();
const cache = new Map<string, PlotData>();

export const overlayActive = () => overlay.items.length > 0;
export const onOverlayChange = (fn: () => void) => listeners.add(fn);
const notify = () => listeners.forEach((f) => f());
/** A user change: re-render and save the selection into the workspace (debounced). */
const changed = () => {
  setUiState({ overlay: { items: overlay.items, norm: overlay.norm, style: overlay.style } });
  notify();
};

const NORMS: HistNorm[] = ["mode", "total", "count"];
const STYLES: OverlayStyle[] = ["contour", "dots"];

/** Load the overlay saved in the workspace's UI state (a newly opened workspace). */
function restore(): void {
  const saved = uiState().overlay as { items?: unknown; norm?: unknown; style?: unknown } | undefined;
  const items = Array.isArray(saved?.items) ? (saved.items as OverlayItem[]) : [];
  overlay.items = items
    .filter((it) => it && typeof it.sampleId === "string" && (it.gateId === null || typeof it.gateId === "string"))
    .map((it) => ({ sampleId: it.sampleId, gateId: it.gateId }))
    .slice(0, MAX_ITEMS);
  overlay.norm = NORMS.includes(saved?.norm as HistNorm) ? (saved!.norm as HistNorm) : "mode";
  overlay.style = STYLES.includes(saved?.style as OverlayStyle) ? (saved!.style as OverlayStyle) : "contour";
}

export function setOverlayOption(patch: Partial<Pick<typeof overlay, "norm" | "style">>): void {
  Object.assign(overlay, patch);
  changed();
}

export function clearOverlay(): void {
  overlay.items = [];
  changed();
}

/** Drop items whose sample or gate has disappeared; any workspace change invalidates the cache. */
export function initOverlay(): void {
  let gen = -1;
  const onWorkspace = () => {
    cache.clear();
    const ws = state.ws;
    const fresh = gen !== workspaceGeneration();
    gen = workspaceGeneration();
    if (fresh) restore();
    const before = overlay.items.length;
    // Gate ids are shared between a sample's tree and its groups' templates.
    overlay.items = overlay.items.filter((it) => ws?.samples.some((s) => s.id === it.sampleId) && (it.gateId === null || !!gate(it.gateId) || sampleGates(it.sampleId).some((g) => g.id === it.gateId)));
    if (fresh) notify();
    else if (overlay.items.length !== before) changed();
  };
  on("workspace", onWorkspace);
  if (state.ws) onWorkspace();
}

const dimKey = (d: GateDim | null) => (d ? `${d.channel}:${d.compensated ? 1 : 0}` : "-");

/** Visible display-space window of the graph (null = full axis). */
export interface ViewRanges { x: [number, number] | null; y: [number, number] | null }

async function fetchItem(it: OverlayItem, x: GateDim, y: GateDim | null, r: ViewRanges): Promise<PlotData> {
  const key = `${it.sampleId}|${it.gateId ?? ""}|${dimKey(x)}|${dimKey(y)}|${r.x ?? ""}|${y ? r.y ?? "" : ""}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const d: PlotData = y
    ? { kind: "2d", plot: await api.plot2d(it.sampleId, it.gateId, x, y, 256, 15000, r.x, r.y) }
    : { kind: "hist", hist: await api.plotHist(it.sampleId, it.gateId, x, 256, r.x) };
  if (cache.size > 64) cache.clear(); // zooming creates many keys
  cache.set(key, d);
  return d;
}

const sampleName = (id: string) => state.ws?.samples.find((s) => s.id === id)?.name ?? id;
const popName = (id: string | null) => gate(id)?.name ?? "All events";

/** Primary (current sample + population) first, then the overlay items. */
export function overlayItems(): OverlayItem[] {
  if (!state.sampleId) return [];
  const primary = { sampleId: state.sampleId, gateId: state.gateId };
  const rest = overlay.items.filter((it) => it.sampleId !== primary.sampleId || it.gateId !== primary.gateId);
  return [primary, ...rest];
}

function labelFor(it: OverlayItem, items: OverlayItem[]): string {
  const oneSample = items.every((o) => o.sampleId === items[0].sampleId);
  const onePop = items.every((o) => o.gateId === items[0].gateId);
  if (oneSample && !onePop) return popName(it.gateId);
  if (onePop && !oneSample) return sampleName(it.sampleId);
  return `${sampleName(it.sampleId)} · ${popName(it.gateId)}`;
}

/** Fetch every series for the current axes (primary first). */
export async function fetchSeries(x: GateDim, y: GateDim | null, ranges: ViewRanges = { x: null, y: null }): Promise<Series[]> {
  const items = overlayItems().filter((it) => isLoaded(it.sampleId));
  const out: Series[] = [];
  for (const [i, it] of items.entries()) {
    out.push({ label: labelFor(it, items), color: GATE_COLORS[i % GATE_COLORS.length], data: await fetchItem(it, x, y, ranges) });
  }
  return out;
}

// ---------------------------------------------------------------- picker

export async function openOverlayPicker(): Promise<void> {
  const ws = state.ws;
  if (!ws || !state.sampleId) return status.error("Select a sample first");
  const curPops = new Set<string>(overlay.items.map((i) => i.gateId ?? ""));
  const curSamples = new Set<string>(overlay.items.map((i) => i.sampleId));
  curPops.add(state.gateId ?? "");
  curSamples.add(state.sampleId);
  const tree = sampleGates(state.sampleId);
  const pops = [{ id: "", label: "All events" }, ...tree.map((g) => ({ id: g.id, label: gatePathIn(tree, g.id) }))];
  const popBoxes = pops.map((p) => {
    const cb = h("input", { type: "checkbox", value: p.id, checked: curPops.has(p.id) });
    return { cb, row: h("label", { class: "check-row" }, cb, ` ${p.label}`) };
  });
  const sampleBoxes = ws.samples.map((s) => {
    const cb = h("input", { type: "checkbox", value: s.id, checked: curSamples.has(s.id), disabled: !isLoaded(s.id) });
    return { cb, row: h("label", { class: `check-row${isLoaded(s.id) ? "" : " disabled"}` }, cb, ` ${s.name}`) };
  });
  const body = h("div", { class: "form overlay-picker" },
    h("p", { class: "muted", text: "Every checked population is drawn for every checked sample, on the current axes." }),
    h("div", { class: "overlay-cols" },
      h("div", { class: "field" }, h("span", { text: "Populations" }), h("div", { class: "check-list" }, ...popBoxes.map((b) => b.row))),
      h("div", { class: "field" }, h("span", { text: "Samples" }), h("div", { class: "check-list" }, ...sampleBoxes.map((b) => b.row)))));
  const r = await modal("Overlay populations", body,
    [{ label: "Remove overlay", value: "clear" }, { label: "Cancel", value: "c" }, { label: "Apply", value: "ok", primary: true }], { wide: true });
  if (r === "clear") return clearOverlay();
  if (r !== "ok") return;
  const gids = popBoxes.filter((b) => b.cb.checked).map((b) => (b.cb.value || null));
  const sids = sampleBoxes.filter((b) => b.cb.checked).map((b) => b.cb.value);
  let items: OverlayItem[] = [];
  for (const sampleId of sids) for (const gateId of gids) items.push({ sampleId, gateId });
  items = items.filter((it) => it.sampleId !== state.sampleId || it.gateId !== state.gateId);
  if (items.length > MAX_ITEMS) {
    status.info(`Overlay limited to ${MAX_ITEMS + 1} series`);
    items = items.slice(0, MAX_ITEMS);
  }
  overlay.items = items;
  changed();
}
