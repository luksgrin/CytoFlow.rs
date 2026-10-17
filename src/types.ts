// TypeScript mirror of the shared JSON types in docs/ipc.md.

export type TransformKind = "linear" | "log" | "asinh" | "logicle" | "biex";

export type Transform =
  | { kind: "linear"; t: number; a: number }
  | { kind: "log"; t: number; m: number }
  | { kind: "asinh"; t: number; m: number; a: number }
  | { kind: "logicle"; t: number; w: number; m: number; a: number }
  // FlowJo biexponential: w = width basis (<= 0, e.g. -10, -100), pos / neg decades.
  | { kind: "biex"; t: number; w: number; pos: number; neg: number };

export interface GateDim {
  channel: string;
  compensated: boolean;
}

export type GateShape =
  | { type: "rectangle"; dims: GateDim[]; min: (number | null)[]; max: (number | null)[] }
  | { type: "polygon"; dims: [GateDim, GateDim]; vertices: [number, number][] }
  | {
      type: "ellipse";
      dims: [GateDim, GateDim];
      center: [number, number];
      a: number;
      b: number;
      angle: number;
    }
  | { type: "boolean"; op: "and" | "or" | "not"; refs: string[] };

export interface Gate {
  shape: GateShape;
  negate: boolean;
  grid?: number | null;
}

export interface GateNode {
  id: string;
  name: string;
  parent: string | null;
  gate: Gate;
  color: string | null;
  /** Only on nodes of a sample's own tree (`sample_gates`). */
  tailored?: boolean;
}

/** Node of a sample's own gate tree. */
export type SampleGateNode = GateNode & { tailored: boolean };

/** A sample group; `gates` is its template tree (default transform space). */
export interface Group {
  id: string;
  name: string;
  sample_ids: string[];
  gates: GateNode[];
}

/** Where a gate edit applies. */
export type GateScope = { group: string } | { sample: string };

/** Id of the built-in group containing every sample. */
export const ALL_SAMPLES = "all";

export interface Parameter {
  name: string;
  stain: string | null;
  range: number;
  bits: number;
  amplification: [number, number];
  gain: number;
}

export interface SampleInfo {
  id: string;
  name: string;
  path: string;
  event_count: number;
  channels: Parameter[];
  keywords: Record<string, string>;
}

export interface CompMatrix {
  channels: string[];
  spillover: number[][];
}

export type CompSource =
  | { source: "none" }
  | { source: "fcs" }
  | { source: "custom"; matrix: CompMatrix };

export interface WorkspaceView {
  version: number;
  name: string;
  samples: SampleInfo[];
  channel_settings: Record<string, { transform: Transform }>;
  /** sample id → channel → per-sample transform override. */
  sample_channel_settings: Record<string, Record<string, { transform: Transform }>>;
  compensation: CompSource;
  /** groups[0] is always "All Samples" (id "all"). */
  groups: Group[];
  /** sample id → that sample's own gate tree. */
  sample_gates: Record<string, SampleGateNode[]>;
  /** The "All Samples" template (= groups[0].gates), kept for compatibility. */
  gates: GateNode[];
  loaded: string[];
  path: string | null;
}

export interface GroupCreateResult {
  id: string;
  workspace: WorkspaceView;
}

export interface PopulationStats {
  gate_id: string | null;
  name: string;
  count: number;
  parent_count: number;
  total: number;
  pct_parent: number;
  pct_total: number;
}

export interface ChannelStats {
  channel: string;
  mean: number;
  median: number;
  sd: number;
  cv: number;
  geo_mean: number | null;
  min: number;
  max: number;
}

export interface Tick {
  pos: number;
  label: string;
  major: boolean;
}

export interface OpenResult {
  workspace: WorkspaceView;
  errors: string[];
}

export interface ImportResult {
  workspace: WorkspaceView;
  warnings: string[];
  missing: { name: string; path: string }[];
}

export interface GateAddResult {
  id: string;
  workspace: WorkspaceView;
}

export interface HistResult {
  bins: number;
  counts: number[];
  max_count: number;
  total: number;
  range: [number, number];
  ticks: Tick[];
  label: string;
}

export interface Plot2DHeader {
  x_label: string;
  y_label: string;
  x_ticks: Tick[];
  y_ticks: Tick[];
  total: number;
  max_count: number;
  bins: number;
}

/** Parsed form of the `plot_2d` binary response. */
export interface Plot2D {
  header: Plot2DHeader;
  bins: number;
  counts: Uint32Array; // index = y*bins + x, origin bottom-left
  n: number;
  x: Float32Array;
  y: Float32Array;
  d: Uint8Array;
}
