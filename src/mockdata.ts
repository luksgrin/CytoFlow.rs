// Synthetic data + transform maths for the browser mock (src/mock.ts).
// Nothing here is used when running inside Tauri.
import type { CompMatrix, Parameter, Tick, Transform, TransformKind } from "./types";

export const D0 = -0.02;
export const D1 = 1.02;
const LN10 = Math.LN10;

export function rng(seed: number): () => number {
  let s = seed | 0;
  return () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function hash(str: string): number {
  let h = 2166136261;
  for (let i = 0; i < str.length; i++) h = Math.imul(h ^ str.charCodeAt(i), 16777619);
  return h >>> 0;
}

export const CHANNELS: [string, string | null][] = [
  ["FSC-A", null], ["FSC-H", null], ["SSC-A", null], ["SSC-H", null],
  ["FITC-A", "CD3"], ["PE-A", "CD4"], ["PerCP-Cy5.5-A", "CD8"], ["PE-Cy7-A", "CD56"],
  ["APC-A", "CD19"], ["APC-Cy7-A", "CD14"], ["BV421-A", "CD45"], ["BV510-A", "Live/Dead"],
  ["Time", null],
];
export const FLUOR = CHANNELS.slice(4, 12).map((c) => c[0]);
const RANGE = 262144;

export function isScatter(name: string): boolean {
  return /^(FSC|SSC|Time)/i.test(name);
}

// ---------------------------------------------------------------- transforms

export function defaultTransform(kind: TransformKind, range: number): Transform {
  switch (kind) {
    case "linear": return { kind, t: range, a: 0 };
    case "log": return { kind, t: range, m: 4.5 };
    case "asinh": return { kind, t: range, m: 4.5, a: 0.3 };
    case "logicle": return { kind, t: range, w: 0.5, m: 4.5, a: 0 };
  }
}

export function initialTransform(ch: string): Transform {
  if (ch === "Time") return defaultTransform("linear", 1024);
  return defaultTransform(isScatter(ch) ? "linear" : "logicle", RANGE);
}

// The mock approximates logicle with an asinh of matching width (good enough for UI testing).
function asinhParams(t: Transform): [number, number, number] {
  if (t.kind === "asinh") return [t.t, t.m, t.a];
  if (t.kind === "logicle") return [t.t, t.m, 2 * t.w + t.a];
  return [1, 1, 0];
}

export function fwd(t: Transform, x: number): number {
  switch (t.kind) {
    case "linear": return (x + t.a) / (t.t + t.a);
    case "log": return Math.log10(Math.max(x, 1e-6) / t.t) / t.m + 1;
    default: {
      const [T, M, A] = asinhParams(t);
      return (Math.asinh((x * Math.sinh(M * LN10)) / T) + A * LN10) / ((M + A) * LN10);
    }
  }
}

export function inv(t: Transform, y: number): number {
  switch (t.kind) {
    case "linear": return y * (t.t + t.a) - t.a;
    case "log": return t.t * Math.pow(10, (y - 1) * t.m);
    default: {
      const [T, M, A] = asinhParams(t);
      return (Math.sinh(y * (M + A) * LN10 - A * LN10) * T) / Math.sinh(M * LN10);
    }
  }
}

const SUP: Record<string, string> = {
  "0": "⁰", "1": "¹", "2": "²", "3": "³", "4": "⁴",
  "5": "⁵", "6": "⁶", "7": "⁷", "8": "⁸", "9": "⁹", "-": "⁻",
};
const sup = (n: number) => String(n).split("").map((c) => SUP[c] ?? c).join("");

export function ticksFor(t: Transform): Tick[] {
  const out: Tick[] = [];
  const inRange = (p: number) => p >= D0 && p <= D1;
  if (t.kind === "linear") {
    const span = t.t;
    const step = Math.pow(10, Math.floor(Math.log10(span / 5)));
    const s = span / step > 25 ? step * 5 : span / step > 10 ? step * 2 : step;
    for (let v = 0; v <= span * 1.02; v += s / 5) {
      const p = fwd(t, v);
      const major = Math.abs(v / s - Math.round(v / s)) < 1e-6;
      if (inRange(p)) out.push({ pos: p, major, label: major ? fmtK(v) : "" });
    }
    return out;
  }
  const maxDec = Math.ceil(Math.log10(t.t));
  const majors: { v: number; label: string; prio: number }[] = [];
  const push = (v: number, major: boolean, label: string, prio = 0) => {
    const p = fwd(t, v);
    if (!inRange(p) || !isFinite(p)) return;
    out.push({ pos: p, major, label: "" });
    if (major) majors.push({ v: p, label, prio });
  };
  if (t.kind !== "log") {
    for (let k = maxDec; k >= 1; k--) {
      for (let m = 9; m >= 2; m--) push(-m * 10 ** (k - 1), false, "");
      push(-(10 ** k), true, `-10${sup(k)}`, 2);
    }
    push(0, true, "0", 0);
  }
  for (let k = 0; k <= maxDec; k++) {
    if (k > 0 || t.kind === "log") push(10 ** k, true, `10${sup(k)}`, 1);
    for (let m = 2; m <= 9; m++) push(m * 10 ** k, false, "");
  }
  // Label majors by priority (0, positive decades, negative decades) without overlaps.
  const shown: number[] = [];
  for (const m of [...majors].sort((a, b) => a.prio - b.prio)) {
    if (shown.some((q) => Math.abs(q - m.v) < 0.05)) continue;
    shown.push(m.v);
    const tk = out.find((o) => o.pos === m.v && o.major);
    if (tk) tk.label = m.label;
  }
  out.sort((a, b) => a.pos - b.pos);
  return out;
}

function fmtK(v: number): string {
  if (v === 0) return "0";
  if (v >= 1000) return `${Math.round(v / 1000)}K`;
  return String(Math.round(v));
}

// ---------------------------------------------------------------- matrices

export function invert(m: number[][]): number[][] {
  const n = m.length;
  const a = m.map((row, i) => [...row, ...Array.from({ length: n }, (_, j) => (i === j ? 1 : 0))]);
  for (let c = 0; c < n; c++) {
    let p = c;
    for (let r = c + 1; r < n; r++) if (Math.abs(a[r][c]) > Math.abs(a[p][c])) p = r;
    if (Math.abs(a[p][c]) < 1e-12) throw "compensation matrix is singular";
    [a[c], a[p]] = [a[p], a[c]];
    const d = a[c][c];
    for (let j = 0; j < 2 * n; j++) a[c][j] /= d;
    for (let r = 0; r < n; r++) {
      if (r === c) continue;
      const f = a[r][c];
      if (f !== 0) for (let j = 0; j < 2 * n; j++) a[r][j] -= f * a[c][j];
    }
  }
  return a.map((row) => row.slice(n));
}

export function demoSpill(): CompMatrix {
  const n = FLUOR.length;
  const s: number[][] = Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => (i === j ? 1 : 0)));
  const set = (a: string, b: string, v: number) => (s[FLUOR.indexOf(a)][FLUOR.indexOf(b)] = v);
  set("FITC-A", "PE-A", 0.18);
  set("PE-A", "PerCP-Cy5.5-A", 0.09);
  set("PE-A", "PE-Cy7-A", 0.04);
  set("PerCP-Cy5.5-A", "PE-Cy7-A", 0.06);
  set("APC-A", "APC-Cy7-A", 0.11);
  set("BV421-A", "BV510-A", 0.14);
  return { channels: [...FLUOR], spillover: s };
}

// ---------------------------------------------------------------- events

interface CellType {
  w: number;
  fsc: [number, number];
  ssc: [number, number];
  pos: Partial<Record<string, number>>; // log10 mean of positive markers
  doublet?: boolean;
}

const TYPES: CellType[] = [
  { w: 0.07, fsc: [16e3, 8e3], ssc: [9e3, 6e3], pos: { "BV510-A": 3.8 } },
  { w: 0.24, fsc: [70e3, 7e3], ssc: [22e3, 5e3], pos: { "FITC-A": 3.6, "PE-A": 3.8, "BV421-A": 4.0 } },
  { w: 0.14, fsc: [68e3, 7e3], ssc: [21e3, 5e3], pos: { "FITC-A": 3.6, "PerCP-Cy5.5-A": 3.9, "BV421-A": 4.0 } },
  { w: 0.09, fsc: [72e3, 7e3], ssc: [20e3, 5e3], pos: { "APC-A": 3.7, "BV421-A": 3.9 } },
  { w: 0.06, fsc: [78e3, 8e3], ssc: [28e3, 6e3], pos: { "PE-Cy7-A": 3.4, "BV421-A": 3.9 } },
  { w: 0.15, fsc: [112e3, 12e3], ssc: [60e3, 10e3], pos: { "APC-Cy7-A": 4.0, "PE-A": 2.8, "BV421-A": 3.7 } },
  { w: 0.19, fsc: [125e3, 14e3], ssc: [140e3, 22e3], pos: { "BV421-A": 3.3 } },
  { w: 0.06, fsc: [95e3, 10e3], ssc: [30e3, 8e3], pos: { "FITC-A": 3.6, "BV421-A": 4.0 }, doublet: true },
];

export interface MockSampleData {
  params: Parameter[];
  raw: Float32Array[]; // per channel, uncompensated
  n: number;
}

export function generateSample(seed: number, n: number, spill: CompMatrix): MockSampleData {
  const r = rng(seed);
  const g = () => {
    const u = Math.max(r(), 1e-12);
    return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * r());
  };
  const shift = (r() - 0.5) * 0.3; // donor-to-donor variation
  const raw = CHANNELS.map(() => new Float32Array(n));
  const ci = (name: string) => CHANNELS.findIndex((c) => c[0] === name);
  const cum: number[] = [];
  TYPES.reduce((acc, t, i) => (cum[i] = acc + t.w), 0);
  const total = cum[cum.length - 1];
  const truth = new Float64Array(FLUOR.length);
  for (let e = 0; e < n; e++) {
    const u = r() * total;
    const t = TYPES[Math.max(0, cum.findIndex((c) => u <= c))];
    const fscA = Math.max(500, t.fsc[0] + g() * t.fsc[1]);
    raw[ci("FSC-A")][e] = fscA;
    raw[ci("FSC-H")][e] = fscA * (t.doublet ? 0.52 : 0.86) * (1 + 0.04 * g());
    const sscA = Math.max(200, t.ssc[0] + g() * t.ssc[1]);
    raw[ci("SSC-A")][e] = sscA;
    raw[ci("SSC-H")][e] = sscA * 0.9 * (1 + 0.05 * g());
    FLUOR.forEach((f, i) => {
      const lm = t.pos[f];
      truth[i] = lm !== undefined ? Math.pow(10, lm + shift + 0.22 * g()) : 40 + 90 * g();
    });
    FLUOR.forEach((f, j) => {
      let v = 0;
      for (let i = 0; i < FLUOR.length; i++) v += truth[i] * spill.spillover[i][j];
      raw[ci(f)][e] = Math.min(RANGE - 1, v + 25 * g());
    });
    raw[ci("Time")][e] = (e / n) * 1000;
  }
  const params: Parameter[] = CHANNELS.map(([name, stain]) => ({
    name, stain, range: name === "Time" ? 1024 : RANGE, bits: 32, amplification: [0, 0], gain: 1,
  }));
  return { params, raw, n };
}
