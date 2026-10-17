// Gate geometry in display space, expressed in the current view's (x, y) order.
import type { GateDim, GateShape } from "../types";
import { D0, D1, clampD } from "./scale";

export type Pt = [number, number];

export type VShape =
  | { t: "rect"; min: [number | null, number | null]; max: [number | null, number | null] }
  | { t: "poly"; v: Pt[] }
  | { t: "ell"; c: Pt; a: number; b: number; ang: number };

/** How gate dims map onto the view: same order, swapped, or 1-D on x / on y. */
export type Mapping = "xy" | "yx" | "x" | "y";

export interface ViewAxes {
  x: GateDim;
  y: GateDim | null; // null = histogram
}

export const dimEq = (a: GateDim, b: GateDim | null) =>
  !!b && a.channel === b.channel && a.compensated === b.compensated;

export function toView(shape: GateShape, ax: ViewAxes): { v: VShape; m: Mapping } | null {
  if (shape.type === "boolean") return null;
  const d = shape.dims;
  if (shape.type === "rectangle" && d.length === 1) {
    if (dimEq(d[0], ax.x)) return { m: "x", v: { t: "rect", min: [shape.min[0], null], max: [shape.max[0], null] } };
    if (ax.y && dimEq(d[0], ax.y)) return { m: "y", v: { t: "rect", min: [null, shape.min[0]], max: [null, shape.max[0]] } };
    return null;
  }
  if (!ax.y || d.length !== 2) return null;
  const m: Mapping | null = dimEq(d[0], ax.x) && dimEq(d[1], ax.y) ? "xy" : dimEq(d[0], ax.y) && dimEq(d[1], ax.x) ? "yx" : null;
  if (!m) return null;
  const sw = m === "yx";
  switch (shape.type) {
    case "rectangle":
      return { m, v: { t: "rect",
        min: sw ? [shape.min[1], shape.min[0]] : [shape.min[0], shape.min[1]],
        max: sw ? [shape.max[1], shape.max[0]] : [shape.max[0], shape.max[1]] } };
    case "polygon":
      return { m, v: { t: "poly", v: shape.vertices.map(([a, b]) => (sw ? [b, a] : [a, b]) as Pt) } };
    case "ellipse":
      return { m, v: { t: "ell", c: sw ? [shape.center[1], shape.center[0]] : [shape.center[0], shape.center[1]],
        a: shape.a, b: shape.b, ang: sw ? Math.PI / 2 - shape.angle : shape.angle } };
  }
}

export function fromView(v: VShape, m: Mapping, orig: GateShape): GateShape {
  if (orig.type === "boolean") return orig;
  const sw = m === "yx";
  if (v.t === "rect" && orig.type === "rectangle") {
    if (m === "x") return { ...orig, min: [v.min[0]], max: [v.max[0]] };
    if (m === "y") return { ...orig, min: [v.min[1]], max: [v.max[1]] };
    return { ...orig, min: sw ? [v.min[1], v.min[0]] : [...v.min], max: sw ? [v.max[1], v.max[0]] : [...v.max] };
  }
  if (v.t === "poly" && orig.type === "polygon")
    return { ...orig, vertices: v.v.map(([a, b]) => (sw ? [b, a] : [a, b]) as [number, number]) };
  if (v.t === "ell" && orig.type === "ellipse")
    return { ...orig, center: sw ? [v.c[1], v.c[0]] : [v.c[0], v.c[1]], a: v.a, b: v.b, angle: sw ? Math.PI / 2 - v.ang : v.ang };
  return orig;
}

export function ellipsePoints(c: Pt, a: number, b: number, ang: number, n = 72): Pt[] {
  const out: Pt[] = [];
  const ca = Math.cos(ang), sa = Math.sin(ang);
  for (let i = 0; i < n; i++) {
    const t = (2 * Math.PI * i) / n;
    const u = a * Math.cos(t), w = b * Math.sin(t);
    out.push([c[0] + u * ca - w * sa, c[1] + u * sa + w * ca]);
  }
  return out;
}

/** Closed outline in display space (open rectangle bounds clamped to the plot range). */
export function outline(v: VShape): Pt[] {
  if (v.t === "poly") return v.v;
  if (v.t === "ell") return ellipsePoints(v.c, v.a, v.b, v.ang);
  const x0 = v.min[0] ?? D0, x1 = v.max[0] ?? D1, y0 = v.min[1] ?? D0, y1 = v.max[1] ?? D1;
  return [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
}

export function contains(v: VShape, p: Pt): boolean {
  if (v.t === "rect") {
    return (v.min[0] === null || p[0] >= v.min[0]) && (v.max[0] === null || p[0] <= v.max[0]) &&
      (v.min[1] === null || p[1] >= v.min[1]) && (v.max[1] === null || p[1] <= v.max[1]);
  }
  if (v.t === "ell") {
    const dx = p[0] - v.c[0], dy = p[1] - v.c[1];
    const c = Math.cos(v.ang), s = Math.sin(v.ang);
    const u = (dx * c + dy * s) / v.a, w = (-dx * s + dy * c) / v.b;
    return u * u + w * w <= 1;
  }
  let inside = false;
  const vs = v.v;
  for (let i = 0, j = vs.length - 1; i < vs.length; j = i++) {
    const [xi, yi] = vs[i], [xj, yj] = vs[j];
    if (yi > p[1] !== yj > p[1] && p[0] < ((xj - xi) * (p[1] - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

export function labelAnchor(v: VShape): Pt {
  if (v.t === "ell") return [v.c[0], v.c[1] + Math.abs(v.b) * 0.2];
  if (v.t === "poly") {
    const n = v.v.length || 1;
    return [v.v.reduce((a, p) => a + p[0], 0) / n, v.v.reduce((a, p) => a + p[1], 0) / n];
  }
  const o = outline(v);
  return [(clampD(o[0][0]) + clampD(o[1][0])) / 2, Math.min(D1 - 0.06, (clampD(o[0][1]) + clampD(o[2][1])) / 2)];
}

// ---------------------------------------------------------------- editing

export interface Handle { id: string; p: Pt }

export function handles(v: VShape): Handle[] {
  if (v.t === "poly") return v.v.map((p, i) => ({ id: `v${i}`, p }));
  if (v.t === "ell") {
    const c = Math.cos(v.ang), s = Math.sin(v.ang);
    return [
      { id: "a+", p: [v.c[0] + v.a * c, v.c[1] + v.a * s] },
      { id: "a-", p: [v.c[0] - v.a * c, v.c[1] - v.a * s] },
      { id: "b+", p: [v.c[0] - v.b * s, v.c[1] + v.b * c] },
      { id: "b-", p: [v.c[0] + v.b * s, v.c[1] - v.b * c] },
    ];
  }
  const xs = ([["x0", v.min[0]], ["x1", v.max[0]]] as [string, number | null][]).filter((e) => e[1] !== null) as [string, number][];
  const ys = ([["y0", v.min[1]], ["y1", v.max[1]]] as [string, number | null][]).filter((e) => e[1] !== null) as [string, number][];
  if (!ys.length) return xs.map(([id, x]) => ({ id, p: [x, 0.5] }));
  if (!xs.length) return ys.map(([id, y]) => ({ id, p: [0.5, y] }));
  const out: Handle[] = [];
  for (const [ix, x] of xs) for (const [iy, y] of ys) out.push({ id: ix + iy, p: [x, y] });
  return out;
}

export function dragHandle(v: VShape, id: string, p: Pt): VShape {
  if (v.t === "poly") {
    const i = Number(id.slice(1));
    return { ...v, v: v.v.map((q, k) => (k === i ? ([clampD(p[0]), clampD(p[1])] as Pt) : q)) };
  }
  if (v.t === "ell") {
    const dx = p[0] - v.c[0], dy = p[1] - v.c[1];
    const r = Math.max(0.005, Math.hypot(dx, dy));
    if (id[0] === "a") {
      const ang = Math.atan2(dy, dx) + (id === "a-" ? Math.PI : 0);
      return { ...v, a: r, ang };
    }
    const proj = Math.abs(-dx * Math.sin(v.ang) + dy * Math.cos(v.ang));
    return { ...v, b: Math.max(0.005, proj) };
  }
  const min: [number | null, number | null] = [...v.min];
  const max: [number | null, number | null] = [...v.max];
  if (id.includes("x0")) min[0] = clampD(p[0]);
  if (id.includes("x1")) max[0] = clampD(p[0]);
  if (id.includes("y0")) min[1] = clampD(p[1]);
  if (id.includes("y1")) max[1] = clampD(p[1]);
  return { t: "rect", min, max };
}

/** Swap inverted bounds after a handle drag so min <= max. */
export function normalize(v: VShape): VShape {
  if (v.t !== "rect") return v;
  const min = [...v.min] as [number | null, number | null];
  const max = [...v.max] as [number | null, number | null];
  for (let k = 0; k < 2; k++) {
    const a = min[k], b = max[k];
    if (a !== null && b !== null && a > b) [min[k], max[k]] = [b, a];
  }
  return { t: "rect", min, max };
}

export function moveShape(v: VShape, dx: number, dy: number): VShape {
  const mv = (x: number | null, d: number) => (x === null ? null : x + d);
  if (v.t === "poly") return { ...v, v: v.v.map(([x, y]) => [x + dx, y + dy] as Pt) };
  if (v.t === "ell") return { ...v, c: [v.c[0] + dx, v.c[1] + dy] };
  return { t: "rect", min: [mv(v.min[0], dx), mv(v.min[1], dy)], max: [mv(v.max[0], dx), mv(v.max[1], dy)] };
}

// ---------------------------------------------------------------- freehand (lasso) simplification

function segDist(p: Pt, a: Pt, b: Pt): number {
  const dx = b[0] - a[0], dy = b[1] - a[1];
  const l2 = dx * dx + dy * dy;
  const t = l2 > 0 ? Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2)) : 0;
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

/** Ramer–Douglas–Peucker on an open polyline (iterative), keeping both ends. */
export function rdp(pts: Pt[], eps: number): Pt[] {
  if (pts.length < 3) return pts.slice();
  const keep = new Uint8Array(pts.length);
  keep[0] = keep[pts.length - 1] = 1;
  const stack: [number, number][] = [[0, pts.length - 1]];
  while (stack.length) {
    const [i, j] = stack.pop()!;
    let best = -1, dmax = eps;
    for (let k = i + 1; k < j; k++) {
      const d = segDist(pts[k], pts[i], pts[j]);
      if (d > dmax) (dmax = d), (best = k);
    }
    if (best < 0) continue;
    keep[best] = 1;
    stack.push([i, best], [best, j]);
  }
  return pts.filter((_, k) => keep[k]);
}

/**
 * Simplify a closed freehand outline to at most `maxVertices` vertices: RDP on
 * the two halves split at the point farthest from the start, growing the
 * tolerance (starting at `eps`) until the vertex budget is met.
 */
export function simplifyClosed(pts: Pt[], maxVertices = 64, eps = 0.75): Pt[] {
  const clean = pts.filter((p, i) => i === 0 || p[0] !== pts[i - 1][0] || p[1] !== pts[i - 1][1]);
  if (clean.length <= 3) return clean;
  let far = 0, dmax = -1;
  clean.forEach((p, i) => {
    const d = Math.hypot(p[0] - clean[0][0], p[1] - clean[0][1]);
    if (d > dmax) (dmax = d), (far = i);
  });
  const ring = [...clean, clean[0]];
  for (let e = eps, iter = 0; iter < 40; e *= 1.4, iter++) {
    const a = rdp(ring.slice(0, far + 1), e);
    const b = rdp(ring.slice(far), e);
    const out = [...a, ...b.slice(1, -1)];
    if (out.length <= maxVertices) return out;
  }
  // Fallback: even decimation.
  const step = clean.length / maxVertices;
  return Array.from({ length: maxVertices }, (_, i) => clean[Math.floor(i * step)]);
}

/** Absolute area of a closed polygon (shoelace). */
export function polygonArea(v: Pt[]): number {
  let a = 0;
  for (let i = 0, j = v.length - 1; i < v.length; j = i++) a += (v[j][0] + v[i][0]) * (v[j][1] - v[i][1]);
  return Math.abs(a / 2);
}
