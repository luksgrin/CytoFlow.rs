// In-memory implementation of the IPC contract (docs/ipc.md) used when the UI
// runs in a plain browser. Same command names, args and return shapes.
import type {
  ChannelStats, CompMatrix, CompSource, GateDim, GateNode, GateShape, HistResult,
  PopulationStats, SampleInfo, Transform, TransformKind, WorkspaceView,
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

function makeSample(path: string): MockSample {
  const id = `s${idCounter++}`;
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

function tf(ch: string): Transform {
  return ws.channel_settings[ch]?.transform ?? initialTransform(ch);
}
const disp = (ch: string, raw: number) => fwd(tf(ch), raw);

function emptyWorkspace(): WorkspaceView {
  const channel_settings: WorkspaceView["channel_settings"] = {};
  CHANNELS.forEach(([c]) => (channel_settings[c] = { transform: initialTransform(c) }));
  return {
    version: 1, name: "Untitled", samples: [], channel_settings,
    compensation: { source: "fcs" }, gates: [], loaded: [], path: null,
  };
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
  }
  return errors;
}

const dim = (channel: string, compensated = false): GateDim => ({ channel, compensated });
function addGate(name: string, parent: string | null, shape: GateShape, color: string | null = null): string {
  const id = `g${idCounter++}`;
  ws.gates.push({ id, name, parent, gate: { shape, negate: false }, color });
  return id;
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
  const t = tf(d.channel);
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
  const g = ws.gates.find((x) => x.id === gateId);
  if (!g) throw `unknown gate ${gateId}`;
  const parent = gateMask(s, g.parent);
  const sm = shapeMask(s, g.gate.shape);
  const out = new Uint8Array(s.data.n);
  for (let i = 0; i < out.length; i++) out[i] = parent[i] && (g.gate.negate ? !sm[i] : sm[i]) ? 1 : 0;
  cache.set(key, out);
  return out;
}

const countMask = (m: Uint8Array) => m.reduce((a, b) => a + b, 0);

function preorder(): GateNode[] {
  const out: GateNode[] = [];
  const walk = (p: string | null) =>
    ws.gates.filter((g) => g.parent === p).forEach((g) => (out.push(g), walk(g.id)));
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
  for (const g of preorder()) {
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

function plotHist(sid: string, gid: string | null, x: GateDim, bins: number): HistResult {
  const s = getSample(sid);
  const v = display(s, x);
  const m = gateMask(s, gid);
  const counts = new Array<number>(bins).fill(0);
  let total = 0;
  for (let i = 0; i < v.length; i++) {
    if (!m[i]) continue;
    total++;
    counts[Math.min(bins - 1, Math.floor(((v[i] - D0) / (D1 - D0)) * bins))]++;
  }
  return { bins, counts, max_count: Math.max(0, ...counts), total, range: [D0, D1], ticks: ticksFor(tf(x.channel)), label: label(x) };
}

function plot2d(sid: string, gid: string | null, x: GateDim, y: GateDim, bins: number, maxPoints: number): ArrayBuffer {
  const s = getSample(sid);
  const vx = display(s, x);
  const vy = display(s, y);
  const m = gateMask(s, gid);
  const counts = new Uint32Array(bins * bins);
  const idx: number[] = [];
  const binOf = (i: number) => {
    const bx = Math.min(bins - 1, Math.floor(((vx[i] - D0) / (D1 - D0)) * bins));
    const by = Math.min(bins - 1, Math.floor(((vy[i] - D0) / (D1 - D0)) * bins));
    return by * bins + bx;
  };
  for (let i = 0; i < vx.length; i++) if (m[i]) (idx.push(i), counts[binOf(i)]++);
  const maxCount = counts.reduce((a, b) => Math.max(a, b), 0);
  const stride = Math.max(1, idx.length / Math.max(1, maxPoints));
  const pick: number[] = [];
  for (let k = 0; k < idx.length && pick.length < maxPoints; k += stride) pick.push(idx[Math.floor(k)]);
  const header = new TextEncoder().encode(JSON.stringify({
    x_label: label(x), y_label: label(y), x_ticks: ticksFor(tf(x.channel)), y_ticks: ticksFor(tf(y.channel)),
    total: idx.length, max_count: maxCount, bins,
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
  pick.forEach((i) => (dv.setFloat32(o, vx[i], true), (o += 4)));
  pick.forEach((i) => (dv.setFloat32(o, vy[i], true), (o += 4)));
  const lmax = Math.log1p(maxCount) || 1;
  pick.forEach((i) => dv.setUint8(o++, Math.round((255 * Math.log1p(counts[binOf(i)])) / lmax)));
  return buf;
}

function remapGates(ch: string, from: Transform, to: Transform): void {
  const r = (v: number) => fwd(to, inv(from, v));
  for (const g of ws.gates) {
    const sh = g.gate.shape;
    if (sh.type === "boolean") continue;
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
}

// ---------------------------------------------------------------- dispatch

type Args = Record<string, unknown>;
const view = () => clone(ws);

function dispatch(cmd: string, a: Args): unknown {
  const str = (k: string) => a[k] as string;
  switch (cmd) {
    case "workspace_new": samples.clear(); ws = emptyWorkspace(); return view();
    case "workspace_get": return view();
    case "workspace_open": demoWorkspace(); ws.path = str("path"); ws.name = basename(str("path")); return { workspace: view(), errors: [] };
    case "workspace_save": ws.path = str("path"); ws.name = basename(str("path")).replace(/\.cfw\.json$/, ""); return view();
    case "workspace_import_wsp": {
      demoWorkspace();
      const dirs = a.searchDirs as string[];
      return {
        workspace: view(), warnings: ["Mock mode: FlowJo import simulated with demo data"],
        missing: dirs.length < 2 ? [{ name: "Donor4_PBMC.fcs", path: "/old/location/Donor4_PBMC.fcs" }] : [],
      };
    }
    case "sample_add": { const errors = addSamples(a.paths as string[]); return { workspace: view(), errors }; }
    case "sample_remove": samples.delete(str("sampleId")); ws.samples = ws.samples.filter((s) => s.id !== str("sampleId")); ws.loaded = ws.loaded.filter((s) => s !== str("sampleId")); return view();
    case "sample_keywords": return { ...(getSample(str("sampleId")) as MockSample & { fullKeywords: Record<string, string> }).fullKeywords };
    case "channel_set_transform": {
      const t = a.transform as Transform;
      remapGates(str("channel"), tf(str("channel")), t);
      ws.channel_settings[str("channel")] = { transform: clone(t) };
      return view();
    }
    case "channel_default_transform": {
      const p = getSample(str("sampleId")).data.params.find((x) => x.name === str("channel"));
      return defaultTransform(a.kind as TransformKind, p?.range ?? 262144);
    }
    case "compensation_set": ws.compensation = clone(a.comp as CompSource); return view();
    case "compensation_matrix": return clone(effectiveMatrix(getSample(str("sampleId"))));
    case "gate_add": {
      const id = addGate(str("name"), (a.parent as string | null) ?? null, clone(a.shape as GateShape));
      ws.gates[ws.gates.length - 1].gate.negate = Boolean(a.negate);
      return { id, workspace: view() };
    }
    case "gate_update": {
      const g = ws.gates.find((x) => x.id === str("gateId"));
      if (!g) throw `unknown gate ${str("gateId")}`;
      if (a.shape !== undefined) g.gate.shape = clone(a.shape as GateShape);
      if (a.name !== undefined) g.name = str("name");
      if (a.negate !== undefined) g.gate.negate = Boolean(a.negate);
      if (a.color !== undefined) g.color = (a.color as string | null) ?? null;
      return view();
    }
    case "gate_remove": {
      const kill = new Set([str("gateId")]);
      let grew = true;
      while (grew) {
        grew = false;
        for (const g of ws.gates) if (g.parent && kill.has(g.parent) && !kill.has(g.id)) (kill.add(g.id), (grew = true));
      }
      ws.gates = ws.gates.filter((g) => !kill.has(g.id));
      return view();
    }
    case "population_stats": return populationStats(str("sampleId"));
    case "channel_stats": return channelStats(str("sampleId"), a.gateId as string | null, str("channel"), Boolean(a.compensated));
    case "axis_ticks": return ticksFor(tf(str("channel")));
    case "plot_hist": return plotHist(str("sampleId"), a.gateId as string | null, a.x as GateDim, a.bins as number);
    case "plot_2d": return plot2d(str("sampleId"), a.gateId as string | null, a.x as GateDim, a.y as GateDim, a.bins as number, a.maxPoints as number);
    case "export_population_csv": return countMask(gateMask(getSample(str("sampleId")), a.gateId as string | null));
    case "export_stats_csv": return ws.samples.length * (ws.gates.length + 1);
    default: throw `unknown command ${cmd}`;
  }
}

const MUTATING = /^(workspace_(new|open|import_wsp)|sample_(add|remove)|channel_set_transform|compensation_set|gate_(add|update|remove))$/;

export async function mockInvoke<T>(cmd: string, args: Args = {}): Promise<T> {
  if (!ws) demoWorkspace();
  await new Promise((r) => setTimeout(r, 4));
  if (MUTATING.test(cmd)) cache.clear();
  const out = dispatch(cmd, args);
  return (out instanceof ArrayBuffer ? out : clone(out)) as T;
}
