// Application state + a tiny event emitter. UI modules subscribe to events and
// re-render; all backend mutations go through `applyWorkspace`.
import { api, setDefaultScope } from "./ipc";
import type { PlotKind } from "./plot/render";
import {
  ALL_SAMPLES, type GateDim, type GateNode, type GateScope, type Group, type PopulationStats,
  type SampleInfo, type Transform, type WorkspaceView,
} from "./types";

export type Tool = "select" | "rect" | "polygon" | "lasso" | "ellipse" | "range" | "quadrant";
export type EventName = "workspace" | "selection" | "axes" | "plotkind" | "tool" | "stats" | "theme" | "view" | "gatesel";

export interface AppState {
  ws: WorkspaceView | null;
  /**
   * Selected group (its template tree is shown and gate edits apply to the
   * group), or null when a single sample is selected (edits tailor it).
   */
  groupId: string | null;
  /** Sample whose data is plotted (and whose tree is shown when groupId is null). */
  sampleId: string | null;
  gateId: string | null; // selected population (null = All events)
  x: GateDim | null;
  y: GateDim | null;
  hist: boolean; // Y axis = Histogram
  plotKind: PlotKind;
  tool: Tool;
  view: "graph" | "batch" | "layout";
  popStats: PopulationStats[]; // for the selected sample
  plotGate: string | null; // gate selected on the plot for editing
}

export const state: AppState = {
  ws: null, groupId: ALL_SAMPLES, sampleId: null, gateId: null, x: null, y: null, hist: false,
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
export const groupById = (id: string | null): Group | null => (id ? state.ws?.groups.find((g) => g.id === id) ?? null : null);
export const group = (): Group | null => groupById(state.groupId);
/** Groups that contain a sample (All Samples first). */
export const groupsOf = (sampleId: string | null): Group[] =>
  sampleId ? state.ws?.groups.filter((g) => g.sample_ids.includes(sampleId)) ?? [] : [];

/** Gate tree of a sample (its own copies, with `tailored` flags). */
export const sampleGates = (sampleId: string | null = state.sampleId): GateNode[] =>
  (sampleId ? state.ws?.sample_gates?.[sampleId] : undefined) ?? [];

/**
 * The tree being viewed and edited: the selected group's template, or the
 * selected sample's own tree.
 */
export function currentGates(): GateNode[] {
  if (!state.ws) return [];
  const g = group();
  return g ? g.gates : sampleGates();
}

/** Where gate edits apply now (passed by default to the gate commands). */
export function currentScope(): GateScope {
  if (state.groupId && group()) return { group: state.groupId };
  if (state.sampleId) return { sample: state.sampleId };
  return { group: ALL_SAMPLES };
}
setDefaultScope(currentScope);

export const gate = (id: string | null): GateNode | null => (id ? currentGates().find((g) => g.id === id) ?? null : null);
export const children = (parent: string | null): GateNode[] => currentGates().filter((g) => g.parent === parent);
/** A gate of a sample's own tree (default: the plotted sample). */
export const sampleGate = (id: string | null, sampleId: string | null = state.sampleId): GateNode | null =>
  id ? sampleGates(sampleId).find((g) => g.id === id) ?? null : null;

/** Effective transform of a channel for a sample (override, else the default). */
export function transformFor(channel: string, sampleId: string | null = state.sampleId): Transform | null {
  const ov = sampleId ? state.ws?.sample_channel_settings?.[sampleId]?.[channel] : undefined;
  return ov?.transform ?? state.ws?.channel_settings[channel]?.transform ?? null;
}
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

/** Path of population names within an explicit tree (e.g. one sample's own tree). */
export function gatePathIn(tree: GateNode[], id: string | null): string {
  const parts: string[] = [];
  let g = id ? tree.find((x) => x.id === id) ?? null : null;
  while (g) {
    parts.unshift(g.name);
    const p = g.parent;
    g = p ? tree.find((x) => x.id === p) ?? null : null;
  }
  return parts.length ? parts.join(" / ") : "All events";
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

// ---------------------------------------------------------------- persisted UI state
//
// Layouts and UI preferences live in the workspace file (`layouts_set`,
// `ui_state_set`). Writes are debounced; `flushPersistence()` sends the
// pending ones right away (before saving or replacing the workspace).

/** Last axes / population shown for a sample. */
export interface SampleView { x: GateDim | null; y: GateDim | null; hist: boolean; gateId: string | null }

/** Shape of `WorkspaceView.ui_state` (owned by the frontend; unknown keys are kept). */
export interface UiState {
  sampleViews?: Record<string, SampleView>;
  selection?: { sampleId: string | null; groupId: string | null };
  [key: string]: unknown;
}

let ui: UiState = {};
let generation = 0;
const pendingWrites = new Map<string, { timer: number; fn: () => Promise<unknown> }>();
const inflight = new Set<Promise<unknown>>();

/** Incremented whenever a different workspace is loaded (open / new / import / boot). */
export const workspaceGeneration = () => generation;
export const uiState = (): UiState => ui;

function track(p: Promise<unknown>): Promise<unknown> {
  const q = p.catch((e) => console.error("persist:", e)).finally(() => inflight.delete(q));
  inflight.add(q);
  return q;
}

/** Run `fn` (a backend write) `ms` after the last call with the same key. */
export function persistLater(key: string, fn: () => Promise<unknown>, ms = 500): void {
  const prev = pendingWrites.get(key);
  if (prev) window.clearTimeout(prev.timer);
  const timer = window.setTimeout(() => {
    pendingWrites.delete(key);
    void track(fn());
  }, ms);
  pendingWrites.set(key, { timer, fn });
}

/** Send every pending debounced write now and wait for all writes in flight. */
export async function flushPersistence(): Promise<void> {
  const fns = [...pendingWrites.values()];
  pendingWrites.clear();
  for (const p of fns) {
    window.clearTimeout(p.timer);
    void track(p.fn());
  }
  while (inflight.size) await Promise.all([...inflight]);
}

/** Merge `patch` into the UI state and save it to the workspace (debounced). */
export function setUiState(patch: Partial<UiState>): void {
  ui = { ...ui, ...patch };
  persistLater("ui_state", async () => {
    const v = await api.uiStateSet(ui);
    if (state.ws) state.ws.ui_state = v.ui_state;
  });
}

const dimOk = (d: GateDim | null | undefined, s: SampleInfo): d is GateDim =>
  !!d && typeof d.channel === "string" && s.channels.some((c) => c.name === d.channel);

/** Remember the current axes / population of the plotted sample and the selection. */
function rememberView(): void {
  const sid = state.sampleId;
  if (!state.ws || !sid) return;
  const views = { ...(ui.sampleViews ?? {}) };
  views[sid] = { x: state.x, y: state.y, hist: state.hist, gateId: state.gateId };
  setUiState({ sampleViews: views, selection: { sampleId: sid, groupId: state.groupId } });
}

/** Restore the remembered axes / population of a sample (if any are still valid). */
function restoreView(sid: string | null): void {
  const s = state.ws?.samples.find((x) => x.id === sid);
  const v = sid ? ui.sampleViews?.[sid] : undefined;
  if (!s || !v) return;
  if (dimOk(v.x, s)) state.x = v.x;
  if (dimOk(v.y, s)) state.y = v.y;
  state.hist = !!v.hist;
  const tree = state.groupId ? groupById(state.groupId)?.gates ?? [] : sampleGates(sid);
  state.gateId = v.gateId && tree.some((g) => g.id === v.gateId) ? v.gateId : null;
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

/**
 * Install a workspace view from the backend. `fresh` marks a different
 * workspace (open / new / import; the first view at boot is always fresh):
 * pending writes are dropped and the saved selection / axes are restored.
 */
export async function applyWorkspace(ws: WorkspaceView, opts: { fresh?: boolean } = {}): Promise<void> {
  const fresh = !!opts.fresh || state.ws === null;
  // Tolerate an older backend / mock view without the group / layout fields.
  ws.layouts ??= [];
  ws.ui_state ??= null;
  ws.groups ??= [{ id: ALL_SAMPLES, name: "All Samples", sample_ids: ws.samples.map((s) => s.id), gates: ws.gates ?? [] }];
  ws.sample_gates ??= Object.fromEntries(ws.samples.map((s) => [s.id, (ws.gates ?? []).map((g) => ({ ...g, tailored: false }))]));
  ws.sample_channel_settings ??= {};
  state.ws = ws;
  if (fresh) {
    for (const p of pendingWrites.values()) window.clearTimeout(p.timer);
    pendingWrites.clear();
    generation++;
    ui = ws.ui_state && typeof ws.ui_state === "object" && !Array.isArray(ws.ui_state) ? { ...(ws.ui_state as UiState) } : {};
    const sel = ui.selection;
    if (sel && ws.samples.some((s) => s.id === sel.sampleId)) state.sampleId = sel.sampleId;
    if (sel && (sel.groupId === null || ws.groups.some((g) => g.id === sel.groupId))) state.groupId = sel.groupId;
  }
  if (!ws.samples.some((s) => s.id === state.sampleId)) state.sampleId = ws.samples[0]?.id ?? null;
  if (state.groupId && !ws.groups.some((g) => g.id === state.groupId)) state.groupId = ALL_SAMPLES;
  if (!state.groupId && !state.sampleId) state.groupId = ALL_SAMPLES;
  const ids = new Set(currentGates().map((g) => g.id));
  if (state.gateId && !ids.has(state.gateId)) state.gateId = null;
  if (state.plotGate && !ids.has(state.plotGate)) state.plotGate = null;
  if (fresh) restoreView(state.sampleId);
  const axesChanged = fixAxes() || fresh;
  emit("workspace");
  if (axesChanged) emit("axes");
  await refreshStats();
}

/** Select a single sample: its own tree is shown and gate edits tailor it. */
export async function selectSample(id: string): Promise<void> {
  if (state.sampleId === id && state.groupId === null) return;
  const sampleChanged = state.sampleId !== id;
  state.sampleId = id;
  state.groupId = null;
  state.plotGate = null;
  if (state.gateId && !sampleGate(state.gateId, id)) state.gateId = null;
  if (sampleChanged) restoreView(id);
  const axesChanged = fixAxes() || sampleChanged;
  rememberView();
  emit("selection");
  if (axesChanged) emit("axes");
  if (sampleChanged) await refreshStats();
}

/**
 * Select a group: its template tree is shown and gate edits apply to the
 * group. The plotted sample stays if it is a member, else the first member.
 */
export async function selectGroup(id: string): Promise<void> {
  const g = groupById(id);
  if (!g || state.groupId === id) return;
  state.groupId = id;
  state.plotGate = null;
  const prev = state.sampleId;
  if (g.sample_ids.length && !g.sample_ids.includes(state.sampleId ?? "")) state.sampleId = g.sample_ids[0];
  if (state.gateId && !g.gates.some((x) => x.id === state.gateId)) state.gateId = null;
  const axesChanged = fixAxes();
  rememberView();
  emit("selection");
  if (axesChanged) emit("axes");
  if (prev !== state.sampleId) await refreshStats();
}

export function selectPopulation(id: string | null): void {
  state.gateId = id;
  state.plotGate = null;
  rememberView();
  emit("selection");
}

export function setAxes(x: GateDim | null, y: GateDim | null, hist = state.hist): void {
  if (x) state.x = x;
  if (y) state.y = y;
  state.hist = hist;
  state.plotGate = null;
  rememberView();
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
  const gates = currentGates();
  const used = new Set(gates.map((g) => g.name));
  let n = gates.length + 1;
  while (used.has(`${prefix} ${n}`)) n++;
  return `${prefix} ${n}`;
}
