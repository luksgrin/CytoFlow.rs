// Linked quadrant gates: four sibling 2-D rectangle gates on the same dims whose
// bounds are each open on one side and share the same X and Y divider values.
// No extra contract fields are needed; the set is recognised from the geometry.
import type { GateDim, GateNode, GateShape } from "../types";
import { dimEq, type Pt, type ViewAxes } from "./gates";
import { D0, D1 } from "./scale";

/** Sign of each quadrant along (dim0, dim1): "p" = above the divider, "m" = below. */
export type QuadSign = "pp" | "mp" | "mm" | "pm";
export const QUAD_SIGNS: QuadSign[] = ["pp", "mp", "mm", "pm"];

export interface QuadSet {
  dims: [GateDim, GateDim];
  /** Divider values in gate-dim order (display space). */
  c: [number, number];
  ids: Record<QuadSign, string>;
}

const EPS = 1e-9;
const isNum = (v: number | null | undefined): v is number => v !== null && v !== undefined;

interface Sig { dims: [GateDim, GateDim]; c: [number, number]; s: QuadSign }

function signature(n: GateNode): Sig | null {
  const sh = n.gate.shape;
  if (sh.type !== "rectangle" || sh.dims.length !== 2) return null;
  const c: number[] = [];
  let s = "";
  for (let k = 0; k < 2; k++) {
    const lo = sh.min[k], hi = sh.max[k];
    if (isNum(lo) && !isNum(hi)) (c.push(lo), (s += "p"));
    else if (!isNum(lo) && isNum(hi)) (c.push(hi), (s += "m"));
    else return null;
  }
  return { dims: [sh.dims[0], sh.dims[1]], c: [c[0], c[1]], s: s as QuadSign };
}

/** Find complete quadrant sets among sibling gates (each gate belongs to at most one set). */
export function findQuadSets(nodes: GateNode[]): QuadSet[] {
  const sigs: { n: GateNode; g: Sig }[] = [];
  for (const n of nodes) {
    const g = signature(n);
    if (g) sigs.push({ n, g });
  }
  const used = new Set<string>();
  const out: QuadSet[] = [];
  for (const a of sigs) {
    if (used.has(a.n.id)) continue;
    const ids: Partial<Record<QuadSign, string>> = { [a.g.s]: a.n.id };
    for (const b of sigs) {
      if (b === a || used.has(b.n.id) || ids[b.g.s]) continue;
      if (!dimEq(a.g.dims[0], b.g.dims[0]) || !dimEq(a.g.dims[1], b.g.dims[1])) continue;
      if (Math.abs(a.g.c[0] - b.g.c[0]) > EPS || Math.abs(a.g.c[1] - b.g.c[1]) > EPS) continue;
      ids[b.g.s] = b.n.id;
    }
    if (QUAD_SIGNS.every((s) => ids[s])) {
      QUAD_SIGNS.forEach((s) => used.add(ids[s]!));
      out.push({ dims: a.g.dims, c: a.g.c, ids: ids as Record<QuadSign, string> });
    }
  }
  return out;
}

/** Map of gate id -> the quadrant set it belongs to. */
export function quadIndex(sets: QuadSet[]): Map<string, QuadSet> {
  const m = new Map<string, QuadSet>();
  for (const q of sets) for (const s of QUAD_SIGNS) m.set(q.ids[s], q);
  return m;
}

export interface QuadView {
  set: QuadSet;
  swap: boolean; // gate dims are (y, x) in this view
  c: Pt; // centre in view (x, y) order
}

export function quadToView(q: QuadSet, ax: ViewAxes): QuadView | null {
  if (!ax.y) return null;
  if (dimEq(q.dims[0], ax.x) && dimEq(q.dims[1], ax.y)) return { set: q, swap: false, c: [q.c[0], q.c[1]] };
  if (dimEq(q.dims[0], ax.y) && dimEq(q.dims[1], ax.x)) return { set: q, swap: true, c: [q.c[1], q.c[0]] };
  return null;
}

/** View-space sign (x then y) of the quadrant with gate-order sign `s`. */
export function viewSign(s: QuadSign, swap: boolean): [number, number] {
  const a = s[0] === "p" ? 1 : -1, b = s[1] === "p" ? 1 : -1;
  return swap ? [b, a] : [a, b];
}

/** Quadrant region (clamped to the plot range) for a view sign and centre. */
export function quadRegion(c: Pt, sx: number, sy: number): Pt[] {
  const x0 = sx > 0 ? c[0] : D0, x1 = sx > 0 ? D1 : c[0];
  const y0 = sy > 0 ? c[1] : D0, y1 = sy > 0 ? D1 : c[1];
  return [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
}

/** New shape for one member gate after the dividers move to `c` (gate-dim order). */
export function movedShape(shape: GateShape, c: [number, number]): GateShape {
  if (shape.type !== "rectangle") return shape;
  const min = [...shape.min], max = [...shape.max];
  for (let k = 0; k < 2; k++) {
    if (isNum(min[k])) min[k] = c[k];
    else max[k] = c[k];
  }
  return { ...shape, min, max };
}
