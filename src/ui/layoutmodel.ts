// Layout editor model: page + panels, persistence and live panel data.
// Persisted in localStorage keyed by workspace name (a `layouts` field in the
// workspace file would be the proper long-term home).
import { api } from "../ipc";
import type { PlotKind, PlotData } from "../plot/render";
import { gate, gatePath, gatePathIn, isLoaded, sampleGates, state } from "../state";
import type { GateDim, PopulationStats } from "../types";

export type PageSize = "a4" | "letter";
export type PanelKind = "plot" | "stats";

export interface Panel {
  id: string;
  kind: PanelKind;
  /** Position and size in page px (96 dpi). */
  x: number;
  y: number;
  w: number;
  h: number;
  title: string;
  sampleId: string;
  sampleName: string; // fallback when ids change between sessions
  gateId: string | null;
  gatePath: string; // fallback for the population
  xDim: GateDim | null;
  yDim: GateDim | null; // null = histogram
  plotKind: PlotKind;
}

export interface Layout {
  version: 1;
  size: PageSize;
  landscape: boolean;
  panels: Panel[];
}

const PAGE: Record<PageSize, [number, number]> = { a4: [794, 1123], letter: [816, 1056] };
export const MARGIN = 36;
export const TITLE_H = 20;
export const CAPTION_H = 30;

export function pageDims(l: Layout): [number, number] {
  const [w, h] = PAGE[l.size];
  return l.landscape ? [h, w] : [w, h];
}

export const emptyLayout = (): Layout => ({ version: 1, size: "a4", landscape: false, panels: [] });

// ---------------------------------------------------------------- persistence

const key = (wsName: string) => `cytoflow.layout:${wsName || "Untitled"}`;

export function loadLayout(wsName: string): Layout | null {
  try {
    const raw = localStorage.getItem(key(wsName));
    if (!raw) return null;
    const l = JSON.parse(raw) as Layout;
    return l && l.version === 1 && Array.isArray(l.panels) ? l : null;
  } catch {
    return null;
  }
}

export function saveLayout(wsName: string, l: Layout): void {
  try {
    localStorage.setItem(key(wsName), JSON.stringify(l));
  } catch {
    /* storage unavailable: layout lives for this session only */
  }
}

// ---------------------------------------------------------------- panels

let idSeq = 0;
export const newPanelId = () => `p${Date.now().toString(36)}${(idSeq++).toString(36)}`;

/** Re-attach a panel to the current workspace's sample / gate (ids may change between sessions). */
export function resolvePanel(p: Panel): void {
  const ws = state.ws;
  if (!ws) return;
  if (!ws.samples.some((s) => s.id === p.sampleId)) {
    const s = ws.samples.find((x) => x.name === p.sampleName);
    if (s) p.sampleId = s.id;
  }
  const tree = sampleGates(p.sampleId);
  if (p.gateId && !tree.some((x) => x.id === p.gateId)) {
    const g = tree.find((x) => gatePathIn(tree, x.id) === p.gatePath);
    p.gateId = g ? g.id : p.gateId;
  }
}

export function panelForCurrent(kind: PanelKind, sampleId = state.sampleId, hist = state.hist): Panel | null {
  const s = state.ws?.samples.find((x) => x.id === sampleId);
  if (!s || (kind === "plot" && !state.x)) return null;
  return {
    id: newPanelId(), kind, x: MARGIN, y: MARGIN, w: 240, h: 240 + TITLE_H + CAPTION_H,
    title: "", sampleId: s.id, sampleName: s.name, gateId: state.gateId, gatePath: gatePath(state.gateId),
    xDim: state.x, yDim: hist ? null : state.y, plotKind: state.plotKind,
  };
}

// ---------------------------------------------------------------- data

export interface PanelData {
  plot: PlotData | null;
  stats: PopulationStats[] | null;
  message?: string;
}

const cache = new Map<string, PanelData>();
export const clearPanelCache = () => cache.clear();

const dimKey = (d: GateDim | null) => (d ? `${d.channel}:${d.compensated ? 1 : 0}` : "-");
const panelKey = (p: Panel, bins: number) => `${p.kind}|${p.sampleId}|${p.gateId ?? ""}|${dimKey(p.xDim)}|${dimKey(p.yDim)}|${bins}`;

export async function fetchPanel(p: Panel): Promise<PanelData> {
  resolvePanel(p);
  if (!state.ws?.samples.some((s) => s.id === p.sampleId)) return { plot: null, stats: null, message: "Sample not in workspace" };
  if (!isLoaded(p.sampleId)) return { plot: null, stats: null, message: "Sample not loaded" };
  if (p.gateId && !gate(p.gateId)) return { plot: null, stats: null, message: "Population was deleted" };
  const bins = Math.max(p.w, p.h) > 300 ? 256 : 128;
  const k = panelKey(p, bins);
  const hit = cache.get(k);
  if (hit) return hit;
  const stats = await api.populationStats(p.sampleId);
  let plot: PlotData | null = null;
  if (p.kind === "plot" && p.xDim) {
    plot = p.yDim
      ? { kind: "2d", plot: await api.plot2d(p.sampleId, p.gateId, p.xDim, p.yDim, bins, 20000) }
      : { kind: "hist", hist: await api.plotHist(p.sampleId, p.gateId, p.xDim, bins) };
  }
  const d = { plot, stats };
  cache.set(k, d);
  return d;
}
