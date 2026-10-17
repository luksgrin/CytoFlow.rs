// Gate display: turns the child gates of a population into drawing primitives
// (shared by the graph window, batch view, layout panels and SVG export).
import { labelAnchor, outline, toView, type Pt, type ViewAxes, type VShape } from "../plot/gates";
import { drawPrims, type Prim } from "../plot/prims";
import { findQuadSets, QUAD_SIGNS, quadRegion, quadToView, viewSign, type QuadSet } from "../plot/quadrants";
import { readTheme } from "../plot/render";
import { D0, D1, type Frame } from "../plot/scale";
import { children, gate } from "../state";
import type { PopulationStats } from "../types";
import { fmtPct, pct } from "./dom";
import { gateColor } from "./workspace";

export interface GateDrawOpts {
  compact?: boolean;
  selected?: string | null;
  /** Live geometry of the gate being edited (view space). */
  editV?: VShape | null;
  /** Live centre (view space) of the quadrant set being dragged, keyed by any member id. */
  quadEdit?: { id: string; c: Pt } | null;
  /** Colour for quadrant dividers and the parent outline (defaults to the theme). */
  lineColor?: string;
  mutedColor?: string;
}

export type StatLookup = (id: string) => PopulationStats | null;

const pctText = (st: PopulationStats | null) => (st ? `${fmtPct(pct(st.count, st.parent_count))}%` : "");

/** Quadrant sets among the children of `parent`. */
export const quadSetsOf = (parent: string | null): QuadSet[] => findQuadSets(children(parent));

export function gatePrims(axes: ViewAxes, parent: string | null, stats: StatLookup, opts: GateDrawOpts = {}): Prim[] {
  const out: Prim[] = [];
  const compact = !!opts.compact;
  const muted = opts.mutedColor ?? readTheme().muted;
  const own = gate(parent);
  const ownView = own ? toView(own.gate.shape, axes) : null;
  if (ownView) out.push({ k: "shape", pts: outline(ownView.v), color: muted, fill: 0, width: 1, dash: [4, 4], stroke: true });

  const quads = quadSetsOf(parent);
  const inQuad = new Set<string>();
  for (const q of quads) for (const s of QUAD_SIGNS) inQuad.add(q.ids[s]);

  for (const node of children(parent)) {
    if (inQuad.has(node.id)) continue;
    const r = toView(node.gate.shape, axes);
    if (!r) continue;
    const sel = opts.selected === node.id;
    const v = sel && opts.editV ? opts.editV : r.v;
    const col = gateColor(node);
    out.push({ k: "shape", pts: outline(v), color: col, fill: sel ? 0.12 : 0.06, width: sel ? 2.2 : compact ? 1 : 1.5, dash: node.gate.negate ? [6, 3] : [], stroke: true });
    const txt = `${compact ? "" : `${node.name} `}${pctText(stats(node.id))}`.trim();
    if (!txt) continue;
    let [ax, ay] = labelAnchor(v);
    if (v.t === "rect" && v.min[1] === null && v.max[1] === null) ay = 0.93;
    out.push({ k: "label", at: [ax, ay], text: txt, color: col, dir: [0, 0] });
  }

  for (const q of quads) {
    const qv = quadToView(q, axes);
    if (!qv) continue;
    const members = QUAD_SIGNS.map((s) => q.ids[s]);
    const live = opts.quadEdit && members.includes(opts.quadEdit.id) ? opts.quadEdit.c : qv.c;
    const lineCol = opts.lineColor ?? readTheme().fg;
    for (const s of QUAD_SIGNS) {
      const node = gate(q.ids[s]);
      if (!node) continue;
      const [sx, sy] = viewSign(s, qv.swap);
      const col = gateColor(node);
      const sel = opts.selected === node.id;
      if (sel || node.gate.negate) out.push({ k: "shape", pts: quadRegion(live, sx, sy), color: col, fill: sel ? 0.12 : 0.04, width: 1, dash: [6, 3], stroke: node.gate.negate });
      const txt = `${compact ? "" : `${node.name} `}${pctText(stats(node.id))}`.trim();
      if (txt) out.push({ k: "label", at: [sx > 0 ? D1 : D0, sy > 0 ? D1 : D0], text: txt, color: col, dir: [-sx, -sy] });
    }
    const w = compact ? 1 : 1.4;
    out.push({ k: "line", a: [live[0], D0], b: [live[0], D1], color: lineCol, width: w });
    out.push({ k: "line", a: [D0, live[1]], b: [D1, live[1]], color: lineCol, width: w });
    if (!compact) out.push({ k: "dot", at: live, color: lineCol, r: 3 });
  }
  return out;
}

/** Draw the child gates of `parent` that match the axes (plus the parent's own outline, faintly). */
export function drawGates(ctx: CanvasRenderingContext2D, f: Frame, axes: ViewAxes, parent: string | null, stats: StatLookup, opts: GateDrawOpts = {}): void {
  drawPrims(ctx, f, gatePrims(axes, parent, stats, opts), !!opts.compact);
}
