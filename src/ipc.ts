// Typed bridge to the Tauri backend (docs/ipc.md). Falls back to an in-memory
// mock (src/mock.ts) when not running inside Tauri.
import type {
  ChannelStats, CompMatrix, CompSource, GateAddResult, GateDim, GateShape, HistResult,
  ImportResult, OpenResult, Plot2D, Plot2DHeader, PopulationStats, Tick, Transform,
  TransformKind, WorkspaceView,
} from "./types";

export type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

export const isTauri: boolean =
  typeof window !== "undefined" &&
  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== undefined;

let impl: InvokeFn | null = null;
let pending = 0;
const busyListeners = new Set<(n: number) => void>();

export function onBusy(fn: (pending: number) => void): void {
  busyListeners.add(fn);
}

async function getImpl(): Promise<InvokeFn> {
  if (impl) return impl;
  if (isTauri) {
    const core = await import("@tauri-apps/api/core");
    impl = core.invoke as InvokeFn;
  } else {
    const m = await import("./mock");
    impl = m.mockInvoke;
  }
  return impl;
}

/** Override the invoke implementation (used by tests). */
export function setInvoke(fn: InvokeFn): void {
  impl = fn;
}

function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  pending++;
  busyListeners.forEach((f) => f(pending));
  try {
    const fn = await getImpl();
    return await fn<T>(cmd, args);
  } catch (e) {
    throw new Error(`${cmd}: ${errorMessage(e)}`);
  } finally {
    pending--;
    busyListeners.forEach((f) => f(pending));
  }
}

// ---------------------------------------------------------------- plot_2d binary

const MAGIC = 0x31504643; // "CFP1"

export function parsePlot2D(raw: unknown): Plot2D {
  let bytes: Uint8Array;
  if (raw instanceof ArrayBuffer) bytes = new Uint8Array(raw);
  else if (ArrayBuffer.isView(raw)) bytes = new Uint8Array(raw.buffer, raw.byteOffset, raw.byteLength);
  else if (Array.isArray(raw)) bytes = Uint8Array.from(raw as number[]);
  else throw new Error("plot_2d: unexpected response type");

  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let o = 0;
  const need = (n: number) => {
    if (o + n > bytes.byteLength) throw new Error("plot_2d: truncated buffer");
  };
  const u32 = () => {
    need(4);
    const v = dv.getUint32(o, true);
    o += 4;
    return v;
  };
  // Copies keep typed arrays aligned regardless of the header length.
  const take = (n: number) => {
    need(n);
    const s = bytes.slice(o, o + n);
    o += n;
    return s;
  };

  if (u32() !== MAGIC) throw new Error("plot_2d: bad magic");
  const headerLen = u32();
  const header = JSON.parse(new TextDecoder().decode(take(headerLen))) as Plot2DHeader;
  const bins = u32();
  const counts = new Uint32Array(take(bins * bins * 4).buffer);
  const n = u32();
  const x = new Float32Array(take(n * 4).buffer);
  const y = new Float32Array(take(n * 4).buffer);
  const d = take(n);
  return { header, bins, counts, n, x, y, d };
}

// ---------------------------------------------------------------- commands

export const api = {
  workspaceNew: () => call<WorkspaceView>("workspace_new"),
  workspaceGet: () => call<WorkspaceView>("workspace_get"),
  workspaceOpen: (path: string) => call<OpenResult>("workspace_open", { path }),
  workspaceSave: (path: string) => call<WorkspaceView>("workspace_save", { path }),
  workspaceImportWsp: (path: string, searchDirs: string[]) =>
    call<ImportResult>("workspace_import_wsp", { path, searchDirs }),
  sampleAdd: (paths: string[]) => call<OpenResult>("sample_add", { paths }),
  sampleRemove: (sampleId: string) => call<WorkspaceView>("sample_remove", { sampleId }),
  sampleKeywords: (sampleId: string) => call<Record<string, string>>("sample_keywords", { sampleId }),
  channelSetTransform: (channel: string, transform: Transform) =>
    call<WorkspaceView>("channel_set_transform", { channel, transform }),
  channelDefaultTransform: (channel: string, kind: TransformKind, sampleId: string) =>
    call<Transform>("channel_default_transform", { channel, kind, sampleId }),
  compensationSet: (comp: CompSource) => call<WorkspaceView>("compensation_set", { comp }),
  compensationMatrix: (sampleId: string) => call<CompMatrix | null>("compensation_matrix", { sampleId }),
  gateAdd: (name: string, parent: string | null, shape: GateShape, negate = false) =>
    call<GateAddResult>("gate_add", { name, parent, shape, negate }),
  gateUpdate: (
    gateId: string,
    patch: { shape?: GateShape; name?: string; negate?: boolean; color?: string | null },
  ) => call<WorkspaceView>("gate_update", { gateId, ...patch }),
  gateRemove: (gateId: string) => call<WorkspaceView>("gate_remove", { gateId }),
  populationStats: (sampleId: string) => call<PopulationStats[]>("population_stats", { sampleId }),
  channelStats: (sampleId: string, gateId: string | null, channel: string, compensated: boolean) =>
    call<ChannelStats>("channel_stats", { sampleId, gateId, channel, compensated }),
  axisTicks: (channel: string) => call<Tick[]>("axis_ticks", { channel }),
  plotHist: (sampleId: string, gateId: string | null, x: GateDim, bins: number) =>
    call<HistResult>("plot_hist", { sampleId, gateId, x, bins }),
  plot2d: async (
    sampleId: string, gateId: string | null, x: GateDim, y: GateDim, bins: number, maxPoints: number,
  ): Promise<Plot2D> =>
    parsePlot2D(await call<unknown>("plot_2d", { sampleId, gateId, x, y, bins, maxPoints })),
  exportPopulationCsv: (
    sampleId: string, gateId: string | null, path: string, channels: string[] | null,
    compensated: boolean, transformed: boolean,
  ) =>
    call<number>("export_population_csv", { sampleId, gateId, path, channels, compensated, transformed }),
  exportStatsCsv: (path: string) => call<number>("export_stats_csv", { path }),
};
