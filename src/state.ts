// Application state + a tiny event emitter. UI modules subscribe to events and
// re-render; all backend mutations go through `applyWorkspace`.
import { api } from "./ipc";
import type { PlotKind } from "./plot/render";
import type { GateDim, GateNode, PopulationStats, SampleInfo, WorkspaceView } from "./types";

export type Tool = "select" | "rect" | "polygon" | "ellipse" | "range" | "quadrant";
export type EventName = "workspace" | "selection" | "axes" | "plotkind" | "tool" | "stats" | "theme" | "view" | "gatesel";

export interface AppState {
  ws: WorkspaceView | null;
  sampleId: string | null;
  gateId: string | null; // selected population (null = All events)
  x: GateDim | null;
  y: GateDim | null;
  hist: boolean; // Y axis = Histogram
  plotKind: PlotKind;
  tool: Tool;
  view: "graph" | "batch";
  popStats: PopulationStats[]; // for the selected sample
  plotGate: string | null; // gate selected on the plot for editing
}

export const state: AppState = {
  ws: null, sampleId: null, gateId: null, x: null, y: null, hist: false,
  plotKind: "pseudo", tool: "select", view: "graph", popStats: [], plotGate: null,
};

const listeners = new Map<EventName, Set<() => void>>();
export function on(ev: EventName, fn: () => void): void {
  if (!listeners.has(ev)) listeners.set(ev, new Set());
  listeners.get(ev)!.add(fn);
}
export function emit(ev: EventName): void {
  listeners.get(ev)?.forEach((fn) => {
    try {
      fn();
    } catch (e) {
      console.error(e);
    }
  });
}

// ---------------------------------------------------------------- selectors

export const sample = (): SampleInfo | null => state.ws?.samples.find((s) => s.id === state.sampleId) ?? null;
export const isLoaded = (id: string | null) => !!id && !!state.ws?.loaded.includes(id);
export const gate = (id: string | null): GateNode | null => (id ? state.ws?.gates.find((g) => g.id === id) ?? null : null);
export const children = (parent: string | null): GateNode[] => state.ws?.gates.filter((g) => g.parent === parent) ?? [];
export const compAvailable = () => !!state.ws && state.ws.compensation.source !== "none";
export const statFor = (gateId: string | null) => state.popStats.find((s) => s.gate_id === gateId) ?? null;
export const isScatterName = (ch: string) => /^(FSC|SSC|Time)/i.test(ch);

export function channelLabel(name: string, s: SampleInfo | null = sample()): string {
  const p = s?.channels.find((c) => c.name === name);
  return p?.stain ? `${p.stain} (${name})` : name;
}
export function shortName(name: string, s: SampleInfo | null = sample()): string {
  return s?.channels.find((c) => c.name === name)?.stain || name;
}

/** Path of population names from root, e.g. "Lymphocytes / Single cells". */
export function gatePath(id: string | null): string {
  const parts: string[] = [];
  let g = gate(id);
  while (g) {
    parts.unshift(g.name);
    g = gate(g.parent);
  }
  return parts.length ? parts.join(" / ") : "All events";
}

// ---------------------------------------------------------------- mutations

let statsSeq = 0;
export async function refreshStats(): Promise<void> {
  const my = ++statsSeq;
  const sid = state.sampleId;
  if (!sid || !isLoaded(sid)) {
    state.popStats = [];
    emit("stats");
    return;
  }
  try {
    const st = await api.populationStats(sid);
    if (my !== statsSeq) return;
    state.popStats = st;
  } catch (e) {
    if (my !== statsSeq) return;
    state.popStats = [];
    throw e;
  } finally {
    if (my === statsSeq) emit("stats");
  }
}

function defaultDim(s: SampleInfo, prefer: string[], fallbackIdx: number): GateDim {
  const names = s.channels.map((c) => c.name);
  const ch = prefer.find((p) => names.includes(p)) ?? names[Math.min(fallbackIdx, names.length - 1)] ?? "";
  return { channel: ch, compensated: compAvailable() && !isScatterName(ch) };
}

/** Keep axes valid for the selected sample and the compensation state. */
function fixAxes(): boolean {
  const s = sample();
  if (!s) return false;
  const names = s.channels.map((c) => c.name);
  let changed = false;
  if (!state.x || !names.includes(state.x.channel)) (state.x = defaultDim(s, ["FSC-A", "FSC-H"], 0)), (changed = true);
  if (!state.y || !names.includes(state.y.channel)) (state.y = defaultDim(s, ["SSC-A", "SSC-H"], 1)), (changed = true);
  if (!compAvailable()) {
    if (state.x.compensated) (state.x = { ...state.x, compensated: false }), (changed = true);
    if (state.y.compensated) (state.y = { ...state.y, compensated: false }), (changed = true);
  }
  return changed;
}

export async function applyWorkspace(ws: WorkspaceView): Promise<void> {
  state.ws = ws;
  if (!ws.samples.some((s) => s.id === state.sampleId)) state.sampleId = ws.samples[0]?.id ?? null;
  if (state.gateId && !ws.gates.some((g) => g.id === state.gateId)) state.gateId = null;
  if (state.plotGate && !ws.gates.some((g) => g.id === state.plotGate)) state.plotGate = null;
  const axesChanged = fixAxes();
  emit("workspace");
  if (axesChanged) emit("axes");
  await refreshStats();
}

export async function selectSample(id: string): Promise<void> {
  if (state.sampleId === id) return;
  state.sampleId = id;
  state.plotGate = null;
  const axesChanged = fixAxes();
  emit("selection");
  if (axesChanged) emit("axes");
  await refreshStats();
}

export function selectPopulation(id: string | null): void {
  state.gateId = id;
  state.plotGate = null;
  emit("selection");
}

export function setAxes(x: GateDim | null, y: GateDim | null, hist = state.hist): void {
  if (x) state.x = x;
  if (y) state.y = y;
  state.hist = hist;
  state.plotGate = null;
  emit("axes");
}

export function setTool(t: Tool): void {
  state.tool = t;
  emit("tool");
}

export function setPlotGate(id: string | null): void {
  state.plotGate = id;
  emit("gatesel");
}

/** Smallest unused "Gate N" name. */
export function nextGateName(prefix = "Gate"): string {
  const used = new Set(state.ws?.gates.map((g) => g.name));
  let n = (state.ws?.gates.length ?? 0) + 1;
  while (used.has(`${prefix} ${n}`)) n++;
  return `${prefix} ${n}`;
}
