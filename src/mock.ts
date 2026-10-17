// In-memory implementation of the IPC contract (docs/ipc.md) used when the UI
// runs in a plain browser. Same command names, args and return shapes,
// including groups (template trees) and per-sample gate trees / transforms.
import type {
  ChannelStats, CompMatrix, CompSource, GateDim, GateNode, GateScope, GateShape, Group, HistResult,
  PopulationStats, SampleGateNode, SampleInfo, StoredLayout, Transform, TransformKind, WorkspaceView,
} from "./types";
import {
  CHANNELS, D0, D1, defaultTransform, demoSpill, fwd, generateSample, hash, initialTransform,
  inv, invert, ticksFor, type MockSampleData,
} from "./mockdata";

interface MockSample { info: SampleInfo; data: MockSampleData; spill: CompMatrix }

let ws: WorkspaceView;
const samples = new Map<string, MockSample>();
const cache = new Map<string, Float32Array | Uint8Array>();
let idCounter = 1;
const EVENTS = 20000;

const basename = (p: string) => p.split(/[\\/]/).pop() ?? p;
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

function makeSample(path: string, id = `s${idCounter++}`): MockSample {
  const spill = demoSpill();
  const data = generateSample(hash(path), EVENTS, spill);
  const name = basename(path);
  const spillStr = [spill.channels.length, ...spill.channels, ...spill.spillover.flat()].join(",");
  const keywords: Record<string, string> = {
    $CYT: "MockCyte 5L", $DATE: "17-OCT-2026", $TOT: String(EVENTS), $PAR: String(CHANNELS.length),
    $FIL: name, $SPILLOVER: spillStr,
  };
  data.params.forEach((p, i) => {
    keywords[`$P${i + 1}N`] = p.name;
    if (p.stain) keywords[`$P${i + 1}S`] = p.stain;
    keywords[`$P${i + 1}R`] = String(p.range);
  });
  const info: SampleInfo = {
    id, name, path, event_count: EVENTS, channels: data.params,
    keywords: { $CYT: keywords.$CYT, $DATE: keywords.$DATE, $TOT: keywords.$TOT, $FIL: name },
  };
  const s = { info, data, spill };
  (s as MockSample & { fullKeywords: Record<string, string> }).fullKeywords = keywords;
  return s;
}

/** Effective transform: the sample's override if any, else the default. */
function tf(ch: string, sid: string | null = null): Transform {
  const ov = sid ? ws.sample_channel_settings[sid]?.[ch]?.transform : undefined;
  return ov ?? ws.channel_settings[ch]?.transform ?? initialTransform(ch);
}
const disp = (ch: string, raw: number) => fwd(tf(ch), raw);

function emptyWorkspace(): WorkspaceView {
  const channel_settings: WorkspaceView["channel_settings"] = {};
  CHANNELS.forEach(([c]) => (channel_settings[c] = { transform: initialTransform(c) }));
  return {
    version: 2, name: "Untitled", samples: [], channel_settings, sample_channel_settings: {},
    compensation: { source: "fcs" },
    groups: [{ id: "all", name: "All Samples", sample_ids: [], gates: [] }],
    sample_gates: {}, gates: [], layouts: [], ui_state: null, loaded: [], path: null,
  };
}

// "Files" written by workspace_save live in localStorage, so a mock workspace
// can be saved, the page reloaded and the workspace reopened (sample data is
// regenerated deterministically from the file path).
const FILE_KEY = (path: string) => `cytoflow.mock.file:${path}`;

function saveFile(path: string): void {
  const doc = clone({ ...ws, gates: undefined, loaded: undefined, path: undefined });
  try {
    localStorage.setItem(FILE_KEY(path), JSON.stringify(doc));
  } catch {
    /* storage unavailable: the save only updates the path */
  }
}

function openFile(path: string): boolean {
  let raw: string | null = null;
  try {
    raw = localStorage.getItem(FILE_KEY(path));
  } catch {
    raw = null;
  }
  if (!raw) return false;
  const doc = JSON.parse(raw) as WorkspaceView;
  samples.clear();
  ws = { ...emptyWorkspace(), ...doc, gates: [], loaded: [], path };
  ws.layouts ??= [];
  ws.ui_state ??= null;
  for (const info of ws.samples) {
    const s = makeSample(info.path, info.id);
    samples.set(info.id, s);
    ws.loaded.push(info.id);
    const n = Number(info.id.replace(/\D+/g, ""));
    if (Number.isFinite(n) && n >= idCounter) idCounter = n + 1;
  }
  for (const g of [...ws.groups, ...Object.values(ws.sample_gates)].flatMap((x) => ("gates" in x ? x.gates : x))) {
    const n = Number(g.id.replace(/\D+/g, ""));
    if (Number.isFinite(n) && n >= idCounter) idCounter = n + 1;
  }
  return true;
}

function addSamples(paths: string[]): string[] {
  const errors: string[] = [];
  for (const p of paths) {
    if (!/\.fcs$/i.test(p)) {
      errors.push(`${basename(p)}: not an FCS file`);
      continue;
    }
    const s = makeSample(p);
    samples.set(s.info.id, s);
    ws.samples.push(s.info);
    ws.loaded.push(s.info.id);
    // New samples join All Samples and receive a copy of its gates.
    ws.groups[0].sample_ids.push(s.info.id);
    ws.sample_gates[s.info.id] = [];
    preorderOf(ws.groups[0].gates).forEach((n) => insertCopy(s.info.id, n));
  }
  return errors;
}

const dim = (channel: string, compensated = false): GateDim => ({ channel, compensated });
function addGate(name: string, parent: string | null, shape: GateShape, color: string | null = null): string {
  return scopedAdd({ group: "all" }, name, parent, shape, false, color);
}

function demoWorkspace(): void {
  samples.clear();
  ws = emptyWorkspace();
  ws.name = "Demo PBMC panel";
  addSamples(["/mock/data/Donor1_PBMC.fcs", "/mock/data/Donor2_PBMC.fcs", "/mock/data/Donor3_Stim.fcs"]);
  const pt = (cx: string, cy: string, x: number, y: number): [number, number] => [disp(cx, x), disp(cy, y)];
  const lym = addGate("Lymphocytes", null, {
    type: "polygon", dims: [dim("FSC-A"), dim("SSC-A")],
    vertices: [pt("FSC-A", "SSC-A", 40e3, 6e3), pt("FSC-A", "SSC-A", 100e3, 8e3), pt("FSC-A", "SSC-A", 105e3, 40e3),
      pt("FSC-A", "SSC-A", 62e3, 46e3), pt("FSC-A", "SSC-A", 38e3, 26e3)],
  }, "#3b82f6");
  const sing = addGate("Single cells", lym, {
    type: "polygon", dims: [dim("FSC-A"), dim("FSC-H")],
    vertices: [pt("FSC-A", "FSC-H", 30e3, 20e3), pt("FSC-A", "FSC-H", 140e3, 100e3),
      pt("FSC-A", "FSC-H", 140e3, 140e3), pt("FSC-A", "FSC-H", 30e3, 32e3)],
  });
  const t = addGate("CD3+ T cells", sing, {
    type: "rectangle", dims: [dim("FITC-A", true)], min: [disp("FITC-A", 900)], max: [null],
  }, "#16a34a");
  addGate("CD4+", t, {
    type: "ellipse", dims: [dim("PE-A", true), dim("PerCP-Cy5.5-A", true)],
    center: [disp("PE-A", 6300), disp("PerCP-Cy5.5-A", 60)], a: 0.12, b: 0.12, angle: 0,
  }, "#e11d48");
  addGate("B cells", sing, {
    type: "rectangle", dims: [dim("APC-A", true), dim("FITC-A", true)],
    min: [disp("APC-A", 900), null], max: [null, disp("FITC-A", 600)],
  }, "#d97706");
  // A "Donors" group, and Donor3's T-cell gate tailored to that sample.
  ws.groups.push({ id: `grp${idCounter++}`, name: "Donors", sample_ids: ws.samples.slice(0, 2).map((s) => s.id), gates: [] });
  const s3 = ws.samples[2]?.id;
  if (s3) scopedUpdate({ sample: s3 }, t, { shape: { type: "rectangle", dims: [dim("FITC-A", true)], min: [disp("FITC-A", 1500)], max: [null] } });
}

// ---------------------------------------------------------------- evaluation

function getSample(id: string): MockSample {
  const s = samples.get(id);
  if (!s) throw `unknown sample ${id}`;
  return s;
}

function effectiveMatrix(s: MockSample): CompMatrix | null {
  const c = ws.compensation;
  return c.source === "none" ? null : c.source === "fcs" ? s.spill : c.matrix;
}

function values(s: MockSample, d: GateDim): Float32Array {
  const idx = s.data.params.findIndex((p) => p.name === d.channel);
  if (idx < 0) throw `channel ${d.channel} not in ${s.info.name}`;
  const m = d.compensated ? effectiveMatrix(s) : null;
  const ci = m ? m.channels.indexOf(d.channel) : -1;
  if (!m || ci < 0) return s.data.raw[idx];
  const key = `c|${s.info.id}|${d.channel}`;
  const hit = cache.get(key);
  if (hit) return hit as Float32Array;
  const invm = invert(m.spillover);
  const cols = m.channels.map((c) => s.data.raw[s.data.params.findIndex((p) => p.name === c)]);
  const out = new Float32Array(s.data.n);
  for (let e = 0; e < s.data.n; e++) {
    let v = 0;
    for (let k = 0; k < cols.length; k++) v += (cols[k]?.[e] ?? 0) * invm[k][ci];
    out[e] = v;
  }
  cache.set(key, out);
  return out;
}

function display(s: MockSample, d: GateDim): Float32Array {
  const key = `d|${s.info.id}|${d.channel}|${d.compensated}`;
  const hit = cache.get(key);
  if (hit) return hit as Float32Array;
  const v = values(s, d);
  const t = tf(d.channel, s.info.id);
  const out = new Float32Array(v.length);
  for (let i = 0; i < v.length; i++) out[i] = Math.min(D1, Math.max(D0, fwd(t, v[i])));
  cache.set(key, out);
  return out;
}

function shapeMask(s: MockSample, shape: GateShape): Uint8Array {
  const n = s.data.n;
  const m = new Uint8Array(n);
  if (shape.type === "rectangle") {
    m.fill(1);
    shape.dims.forEach((d, k) => {
      const v = display(s, d);
      const lo = shape.min[k] ?? -Infinity;
      const hi = shape.max[k] ?? Infinity;
      for (let i = 0; i < n; i++) if (v[i] < lo || v[i] > hi) m[i] = 0;
    });
  } else if (shape.type === "polygon") {
    const x = display(s, shape.dims[0]);
    const y = display(s, shape.dims[1]);
    const vs = shape.vertices;
    for (let i = 0; i < n; i++) {
      let inside = false;
      for (let a = 0, b = vs.length - 1; a < vs.length; b = a++) {
        const [xa, ya] = vs[a];
        const [xb, yb] = vs[b];
        if (ya > y[i] !== yb > y[i] && x[i] < ((xb - xa) * (y[i] - ya)) / (yb - ya) + xa) inside = !inside;
      }
      m[i] = inside ? 1 : 0;
    }
  } else if (shape.type === "ellipse") {
    const x = display(s, shape.dims[0]);
    const y = display(s, shape.dims[1]);
    const c = Math.cos(shape.angle);
    const sn = Math.sin(shape.angle);
    for (let i = 0; i < n; i++) {
      const dx = x[i] - shape.center[0];
      const dy = y[i] - shape.center[1];
      const u = (dx * c + dy * sn) / shape.a;
      const w = (-dx * sn + dy * c) / shape.b;
      m[i] = u * u + w * w <= 1 ? 1 : 0;
    }
  } else {
    const refs = shape.refs.map((r) => gateMask(s, r));
    for (let i = 0; i < n; i++) {
      if (shape.op === "and") m[i] = refs.every((r) => r[i]) ? 1 : 0;
      else if (shape.op === "or") m[i] = refs.some((r) => r[i]) ? 1 : 0;
      else m[i] = refs.some((r) => r[i]) ? 0 : 1;
    }
  }
  return m;
}

function gateMask(s: MockSample, gateId: string | null): Uint8Array {
  if (gateId === null) return new Uint8Array(s.data.n).fill(1);
  const key = `m|${s.info.id}|${gateId}`;
  const hit = cache.get(key);
  if (hit) return hit as Uint8Array;
  const g = treeOf(s.info.id).find((x) => x.id === gateId);
  if (!g) throw `gate not found: ${gateId}`;
  const parent = gateMask(s, g.parent);
  const sm = shapeMask(s, g.gate.shape);
  const out = new Uint8Array(s.data.n);
  for (let i = 0; i < out.length; i++) out[i] = parent[i] && (g.gate.negate ? !sm[i] : sm[i]) ? 1 : 0;
  cache.set(key, out);
  return out;
}

const countMask = (m: Uint8Array) => m.reduce((a, b) => a + b, 0);

function preorderOf<T extends GateNode>(nodes: T[]): T[] {
  const out: T[] = [];
  const walk = (p: string | null) =>
    nodes.filter((g) => g.parent === p).forEach((g) => (out.push(g), walk(g.id)));
  walk(null);
  return out;
}

function populationStats(sid: string): PopulationStats[] {
  const s = getSample(sid);
  const total = s.data.n;
  const counts = new Map<string | null, number>([[null, total]]);
  const rows: PopulationStats[] = [{
    gate_id: null, name: "All events", count: total, parent_count: total, total, pct_parent: 100, pct_total: 100,
  }];
  for (const g of preorderOf(treeOf(sid))) {
    const c = countMask(gateMask(s, g.id));
    counts.set(g.id, c);
    const pc = counts.get(g.parent) ?? total;
    rows.push({
      gate_id: g.id, name: g.name, count: c, parent_count: pc, total,
      pct_parent: pc ? (100 * c) / pc : 0, pct_total: (100 * c) / total,
    });
  }
  return rows;
}

function channelStats(sid: string, gid: string | null, channel: string, compensated: boolean): ChannelStats {
  const s = getSample(sid);
  const v = values(s, dim(channel, compensated));
  const m = gateMask(s, gid);
  const sel: number[] = [];
  for (let i = 0; i < v.length; i++) if (m[i]) sel.push(v[i]);
  const n = sel.length;
  if (!n) return { channel, mean: 0, median: 0, sd: 0, cv: 0, geo_mean: null, min: 0, max: 0 };
  const sorted = Float64Array.from(sel).sort();
  const mean = sel.reduce((a, b) => a + b, 0) / n;
  const sd = Math.sqrt(sel.reduce((a, b) => a + (b - mean) ** 2, 0) / Math.max(1, n - 1));
  const pos = sel.filter((x) => x > 0);
  const median = n % 2 ? sorted[(n - 1) / 2] : (sorted[n / 2 - 1] + sorted[n / 2]) / 2;
  return {
    channel, mean, median, sd, cv: mean ? (100 * sd) / Math.abs(mean) : 0,
    geo_mean: pos.length ? Math.exp(pos.reduce((a, b) => a + Math.log(b), 0) / pos.length) : null,
    min: sorted[0], max: sorted[n - 1],
  };
}

function label(d: GateDim): string {
  const p = CHANNELS.find((c) => c[0] === d.channel);
  return `${d.compensated ? "Comp-" : ""}${d.channel}${p?.[1] ? ` :: ${p[1]}` : ""}`;
}

type Win = [number, number];
/** Same rule as the backend's `plot::view_range`. */
const viewRange = (r: unknown): Win =>
  Array.isArray(r) && r.length === 2 && Number.isFinite(r[0]) && Number.isFinite(r[1]) && r[1] - r[0] > 1e-6 ? [r[0], r[1]] : [D0, D1];

/** Bin index of `v` in window `r` (null = outside; events beyond an outer edge pile into the edge cell). */
function binner(r: Win, bins: number): (v: number) => number | null {
  const pileLo = r[0] <= D0 + 1e-6, pileHi = r[1] >= D1 - 1e-6;
  const scale = bins / (r[1] - r[0]);
  return (v) => {
    if (!Number.isFinite(v) || (v < r[0] && !pileLo) || (v > r[1] && !pileHi)) return null;
    return Math.min(bins - 1, Math.max(0, Math.floor((v - r[0]) * scale)));
  };
}

function plotHist(sid: string, gid: string | null, x: GateDim, bins: number, range: unknown): HistResult {
  const s = getSample(sid);
  const v = display(s, x);
  const m = gateMask(s, gid);
  const r = viewRange(range);
  const bin = binner(r, bins);
  const counts = new Array<number>(bins).fill(0);
  let total = 0;
  for (let i = 0; i < v.length; i++) {
    if (!m[i]) continue;
    const b = bin(v[i]);
    if (b === null) continue;
    total++;
    counts[b]++;
  }
  return { bins, counts, max_count: Math.max(0, ...counts), total, range: r, ticks: ticksFor(tf(x.channel, sid)), label: label(x) };
}

function plot2d(sid: string, gid: string | null, x: GateDim, y: GateDim, bins: number, maxPoints: number, xRange: unknown, yRange: unknown): ArrayBuffer {
  const s = getSample(sid);
  const vx = display(s, x);
  const vy = display(s, y);
  const m = gateMask(s, gid);
  const xr = viewRange(xRange), yr = viewRange(yRange);
  const bx = binner(xr, bins), by = binner(yr, bins);
  const counts = new Uint32Array(bins * bins);
  const idx: number[] = [];
  const cell = new Int32Array(vx.length);
  const binOf = (i: number) => cell[i];
  for (let i = 0; i < vx.length; i++) {
    if (!m[i]) continue;
    const a = bx(vx[i]), b = by(vy[i]);
    if (a === null || b === null) continue;
    idx.push(i);
    cell[i] = b * bins + a;
    counts[b * bins + a]++;
  }
  const maxCount = counts.reduce((a, b) => Math.max(a, b), 0);
  const stride = Math.max(1, idx.length / Math.max(1, maxPoints));
  const pick: number[] = [];
  for (let k = 0; k < idx.length && pick.length < maxPoints; k += stride) pick.push(idx[Math.floor(k)]);
  const header = new TextEncoder().encode(JSON.stringify({
    x_label: label(x), y_label: label(y), x_ticks: ticksFor(tf(x.channel, sid)), y_ticks: ticksFor(tf(y.channel, sid)),
    total: idx.length, max_count: maxCount, bins, x_range: xr, y_range: yr,
  }));
  const n = pick.length;
  const size = 8 + header.length + 4 + bins * bins * 4 + 4 + n * 9;
  const buf = new ArrayBuffer(size);
  const dv = new DataView(buf);
  let o = 0;
  const u32 = (v: number) => (dv.setUint32(o, v, true), (o += 4));
  u32(0x31504643);
  u32(header.length);
  new Uint8Array(buf, o, header.length).set(header);
  o += header.length;
  u32(bins);
  counts.forEach((c) => u32(c));
  u32(n);
  const cl = (v: number, r: Win) => Math.min(r[1], Math.max(r[0], v));
  pick.forEach((i) => (dv.setFloat32(o, cl(vx[i], xr), true), (o += 4)));
  pick.forEach((i) => (dv.setFloat32(o, cl(vy[i], yr), true), (o += 4)));
  const lmax = Math.log1p(maxCount) || 1;
  pick.forEach((i) => dv.setUint8(o++, Math.round((255 * Math.log1p(counts[binOf(i)])) / lmax)));
  return buf;
}

function remapShape(sh: GateShape, ch: string, from: Transform, to: Transform): void {
  const r = (v: number) => fwd(to, inv(from, v));
  if (sh.type === "boolean") return;
  sh.dims.forEach((d, k) => {
    if (d.channel !== ch) return;
    if (sh.type === "rectangle") {
      const lo = sh.min[k], hi = sh.max[k];
      if (lo !== null) sh.min[k] = r(lo);
      if (hi !== null) sh.max[k] = r(hi);
    } else if (sh.type === "polygon") sh.vertices.forEach((v) => (v[k] = r(v[k])));
    else sh.center[k] = r(sh.center[k]);
  });
}

// ---------------------------------------------------------------- groups & per-sample trees

const ALL = "all";
const groupOf = (id: string): Group => {
  const g = ws.groups.find((x) => x.id === id);
  if (!g) throw `group not found: ${id}`;
  return g;
};
const treeOf = (sid: string): SampleGateNode[] => {
  const t = ws.sample_gates[sid];
  if (!t) throw `unknown sample ${sid}`;
  return t;
};
const hasOverride = (sid: string, ch: string) => !!ws.sample_channel_settings[sid]?.[ch];
const dimsOf = (shape: GateShape): GateDim[] => (shape.type === "boolean" ? [] : shape.dims);

/** Re-express a shape between the default space and a sample's space. */
function convert(sid: string, shape: GateShape, toSample: boolean): GateShape {
  const out = clone(shape);
  for (const ch of new Set(dimsOf(shape).map((d) => d.channel))) {
    if (!hasOverride(sid, ch)) continue;
    const [from, to] = toSample ? [tf(ch), tf(ch, sid)] : [tf(ch, sid), tf(ch)];
    remapShape(out, ch, from, to);
  }
  return out;
}

function subtree(nodes: GateNode[], id: string): Set<string> {
  const kill = new Set([id]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const g of nodes) if (g.parent && kill.has(g.parent) && !kill.has(g.id)) (kill.add(g.id), (grew = true));
  }
  return kill;
}

/** Copy a template node into a sample unless it has it already or lacks the parent. */
function insertCopy(sid: string, node: GateNode): void {
  const tree = ws.sample_gates[sid];
  if (!tree || tree.some((n) => n.id === node.id)) return;
  if (node.parent && !tree.some((n) => n.id === node.parent)) return;
  tree.push({ ...clone(node), gate: { ...clone(node.gate), shape: convert(sid, node.gate.shape, true) }, tailored: false });
}

function scopedAdd(scope: GateScope | undefined, name: string, parent: string | null, shape: GateShape, negate: boolean, color: string | null = null): string {
  const sc = scope ?? { group: ALL };
  const id = `g${idCounter++}`;
  const node: GateNode = { id, name, parent, gate: { shape: clone(shape), negate }, color };
  if ("group" in sc) {
    const g = groupOf(sc.group);
    if (parent && !g.gates.some((x) => x.id === parent)) throw `gate not found: ${parent}`;
    g.gates.push(node);
    g.sample_ids.forEach((sid) => insertCopy(sid, node));
  } else {
    const tree = treeOf(sc.sample);
    if (parent && !tree.some((x) => x.id === parent)) throw `gate not found: ${parent}`;
    tree.push({ ...node, tailored: true });
  }
  return id;
}

interface Patch { shape?: GateShape; name?: string; negate?: boolean; color?: string | null }
function patchNode(g: GateNode, p: Patch): void {
  if (p.shape !== undefined) g.gate.shape = clone(p.shape);
  if (p.name !== undefined) g.name = p.name;
  if (p.negate !== undefined) g.gate.negate = Boolean(p.negate);
  if (p.color !== undefined) g.color = p.color ?? null;
}

function scopedUpdate(scope: GateScope | undefined, id: string, p: Patch): void {
  const sc = scope ?? { group: ALL };
  if ("group" in sc) {
    const g = groupOf(sc.group);
    const t = g.gates.find((x) => x.id === id);
    if (!t) throw `gate not found: ${id}`;
    patchNode(t, p);
    for (const sid of g.sample_ids) {
      const n = ws.sample_gates[sid]?.find((x) => x.id === id);
      if (!n) continue;
      if (n.tailored) patchNode(n, { name: p.name, color: p.color });
      else patchNode(n, { ...p, shape: p.shape ? convert(sid, p.shape, true) : undefined });
    }
  } else {
    const n = treeOf(sc.sample).find((x) => x.id === id);
    if (!n) throw `gate not found: ${id}`;
    if (p.shape !== undefined || (p.negate !== undefined && p.negate !== n.gate.negate) || (p.name !== undefined && p.name !== n.name)) n.tailored = true;
    patchNode(n, p);
  }
}

function scopedRemove(scope: GateScope | undefined, id: string): void {
  const sc = scope ?? { group: ALL };
  if ("group" in sc) {
    const g = groupOf(sc.group);
    if (!g.gates.some((x) => x.id === id)) throw `gate not found: ${id}`;
    const kill = subtree(g.gates, id);
    g.gates = g.gates.filter((x) => !kill.has(x.id));
    for (const sid of g.sample_ids) {
      const tree = ws.sample_gates[sid];
      if (!tree?.some((x) => x.id === id)) continue;
      const k = subtree(tree, id);
      ws.sample_gates[sid] = tree.filter((x) => !k.has(x.id));
    }
  } else {
    const tree = treeOf(sc.sample);
    if (!tree.some((x) => x.id === id)) throw `gate not found: ${id}`;
    const k = subtree(tree, id);
    ws.sample_gates[sc.sample] = tree.filter((x) => !k.has(x.id));
  }
}

function copyToGroup(gateId: string, sid: string, groupId: string): void {
  const g = groupOf(groupId);
  const src = treeOf(sid).find((x) => x.id === gateId);
  if (!src) throw `gate not found: ${gateId}`;
  const node: GateNode = { id: src.id, name: src.name, parent: src.parent, color: src.color, gate: { ...clone(src.gate), shape: convert(sid, src.gate.shape, false) } };
  const t = g.gates.find((x) => x.id === gateId);
  if (t) Object.assign(t, clone(node));
  else {
    if (node.parent && !g.gates.some((x) => x.id === node.parent)) throw `the parent population of ${node.name} is not in group ${g.name}`;
    g.gates.push(clone(node));
  }
  for (const m of new Set([...g.sample_ids, sid])) {
    const tree = ws.sample_gates[m];
    if (!tree) continue;
    const n = tree.find((x) => x.id === gateId);
    if (n) {
      n.name = node.name;
      n.color = node.color;
      n.gate = { ...clone(node.gate), shape: convert(m, node.gate.shape, true) };
      n.tailored = false;
    } else insertCopy(m, node);
  }
}

function setMembers(groupId: string, ids: string[]): void {
  if (groupId === ALL) throw "the All Samples group always contains every sample";
  const g = groupOf(groupId);
  const added = ids.filter((s) => !g.sample_ids.includes(s));
  g.sample_ids = [...new Set(ids)].filter((s) => ws.samples.some((x) => x.id === s));
  for (const sid of added) for (const n of preorderOf(g.gates)) insertCopy(sid, n);
}

function setTransform(ch: string, t: Transform, sid: string | null): void {
  if (sid) {
    if (!ws.sample_gates[sid]) throw `unknown sample ${sid}`;
    const old = tf(ch, sid);
    const isDefault = JSON.stringify(t) === JSON.stringify(tf(ch));
    if (isDefault) delete ws.sample_channel_settings[sid]?.[ch];
    else (ws.sample_channel_settings[sid] ??= {})[ch] = { transform: clone(t) };
    ws.sample_gates[sid].forEach((g) => remapShape(g.gate.shape, ch, old, t));
    return;
  }
  const old = tf(ch);
  ws.groups.forEach((g) => g.gates.forEach((n) => remapShape(n.gate.shape, ch, old, t)));
  for (const s of ws.samples) if (!hasOverride(s.id, ch)) ws.sample_gates[s.id]?.forEach((n) => remapShape(n.gate.shape, ch, old, t));
  ws.channel_settings[ch] = { transform: clone(t) };
}

// ---------------------------------------------------------------- dispatch

type Args = Record<string, unknown>;
const view = () => clone({ ...ws, gates: ws.groups[0]?.gates ?? [] });

function dispatch(cmd: string, a: Args): unknown {
  const str = (k: string) => a[k] as string;
  switch (cmd) {
    case "workspace_new": samples.clear(); ws = emptyWorkspace(); return view();
    case "workspace_get": return view();
    case "workspace_open":
      if (!openFile(str("path"))) {
        demoWorkspace();
        ws.path = str("path");
        ws.name = basename(str("path"));
      }
      return { workspace: view(), errors: [] };
    case "workspace_save":
      ws.path = str("path");
      ws.name = basename(str("path")).replace(/\.cfw\.json$/, "");
      saveFile(str("path"));
      return view();
    case "layouts_get": return clone(ws.layouts);
    case "layouts_set": {
      const ls = a.layouts as StoredLayout[];
      if (!Array.isArray(ls) || ls.some((l) => typeof l?.id !== "string" || typeof l?.name !== "string")) throw "invalid layouts";
      ws.layouts = clone(ls).map((l) => ({ id: l.id, name: l.name, data: l.data ?? null }));
      return view();
    }
    case "ui_state_get": return clone(ws.ui_state ?? null);
    case "ui_state_set": ws.ui_state = clone(a.uiState ?? null); return view();
    case "workspace_import_wsp": {
      demoWorkspace();
      const dirs = a.searchDirs as string[];
      return {
        workspace: view(), warnings: ["Mock mode: FlowJo import simulated with demo data"],
        missing: dirs.length < 2 ? [{ name: "Donor4_PBMC.fcs", path: "/old/location/Donor4_PBMC.fcs" }] : [],
      };
    }
    case "sample_add": { const errors = addSamples(a.paths as string[]); return { workspace: view(), errors }; }
    case "sample_remove": {
      const sid = str("sampleId");
      samples.delete(sid);
      ws.samples = ws.samples.filter((s) => s.id !== sid);
      ws.loaded = ws.loaded.filter((s) => s !== sid);
      ws.groups.forEach((g) => (g.sample_ids = g.sample_ids.filter((s) => s !== sid)));
      delete ws.sample_gates[sid];
      delete ws.sample_channel_settings[sid];
      return view();
    }
    case "sample_keywords": return { ...(getSample(str("sampleId")) as MockSample & { fullKeywords: Record<string, string> }).fullKeywords };
    case "channel_set_transform": setTransform(str("channel"), a.transform as Transform, (a.sampleId as string | undefined) ?? null); return view();
    case "channel_default_transform": {
      const p = getSample(str("sampleId")).data.params.find((x) => x.name === str("channel"));
      return defaultTransform(a.kind as TransformKind, p?.range ?? 262144);
    }
    case "compensation_set": ws.compensation = clone(a.comp as CompSource); return view();
    case "compensation_matrix": return clone(effectiveMatrix(getSample(str("sampleId"))));
    case "gate_add": {
      const id = scopedAdd(a.scope as GateScope | undefined, str("name"), (a.parent as string | null) ?? null, a.shape as GateShape, Boolean(a.negate));
      return { id, workspace: view() };
    }
    case "gate_update": scopedUpdate(a.scope as GateScope | undefined, str("gateId"), a as Patch); return view();
    case "gate_remove": scopedRemove(a.scope as GateScope | undefined, str("gateId")); return view();
    case "gate_copy_to_group": copyToGroup(str("gateId"), str("fromSampleId"), str("groupId")); return view();
    case "group_list": return clone(ws.groups);
    case "group_create": {
      const name = str("name").trim();
      if (!name) throw "group name must not be empty";
      const id = `grp${idCounter++}`;
      ws.groups.push({ id, name, sample_ids: [], gates: [] });
      setMembers(id, a.sampleIds as string[]);
      return { id, workspace: view() };
    }
    case "group_set_members": setMembers(str("groupId"), a.sampleIds as string[]); return view();
    case "group_remove": {
      if (str("groupId") === ALL) throw "the All Samples group cannot be removed";
      groupOf(str("groupId"));
      ws.groups = ws.groups.filter((g) => g.id !== str("groupId"));
      return view();
    }
    case "population_stats": return populationStats(str("sampleId"));
    case "channel_stats": return channelStats(str("sampleId"), a.gateId as string | null, str("channel"), Boolean(a.compensated));
    case "axis_ticks": return ticksFor(tf(str("channel"), (a.sampleId as string | undefined) ?? null));
    case "plot_hist": return plotHist(str("sampleId"), a.gateId as string | null, a.x as GateDim, a.bins as number, a.range);
    case "plot_2d": return plot2d(str("sampleId"), a.gateId as string | null, a.x as GateDim, a.y as GateDim, a.bins as number, a.maxPoints as number, a.xRange, a.yRange);
    case "export_population_csv":
    case "export_population": {
      const fmt = (a.format as string | undefined) ?? "csv";
      if (!["csv", "xlsx", "parquet"].includes(fmt)) throw new Error(`unknown format ${fmt}`);
      const n = countMask(gateMask(getSample(str("sampleId")), a.gateId as string | null));
      // Recorded so headless tests can check what the dialog sent.
      (window as unknown as { __mockExports?: unknown[] }).__mockExports ??= [];
      (window as unknown as { __mockExports: unknown[] }).__mockExports.push({ cmd, ...clone(a), format: fmt, rows: n });
      return n;
    }
    case "export_stats_csv": return ws.samples.reduce((n, s) => n + (ws.sample_gates[s.id]?.length ?? 0) + 1, 0);
    default: throw `unknown command ${cmd}`;
  }
}

const MUTATING = /^(workspace_(new|open|import_wsp)|sample_(add|remove)|channel_set_transform|compensation_set|gate_(add|update|remove|copy_to_group)|group_(create|set_members|remove))$/;

export async function mockInvoke<T>(cmd: string, args: Args = {}): Promise<T> {
  if (!ws) demoWorkspace();
  await new Promise((r) => setTimeout(r, 4));
  if (MUTATING.test(cmd)) cache.clear();
  const out = dispatch(cmd, args);
  return (out instanceof ArrayBuffer ? out : clone(out)) as T;
}
