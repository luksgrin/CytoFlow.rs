// Gate overlay: draws child gates on the plot, drawing tools and gate editing.
import { api } from "../ipc";
import {
  contains, dragHandle, fromView, handles, labelAnchor, type Mapping, moveShape, normalize, outline,
  type Pt, toView, type ViewAxes, type VShape,
} from "../plot/gates";
import { readTheme } from "../plot/render";
import { clampD, type Frame, fromPx, fromPy, toPx, toPy } from "../plot/scale";
import {
  applyWorkspace, children, gate, nextGateName, selectPopulation, setPlotGate, shortName, state, statFor,
} from "../state";
import type { GateDim, GateNode, GateShape, PopulationStats } from "../types";
import { chooseColor, deleteGate, renameGate, toggleNegate } from "./actions";
import { fmtPct, pct } from "./dom";
import type { GraphCtx } from "./graph";
import { promptText, showMenu } from "./modal";
import { gateColor } from "./workspace";
import { run, status } from "./status";

type Draft =
  | { t: "rect"; p0: Pt; p1: Pt }
  | { t: "ellipse"; p0: Pt; p1: Pt }
  | { t: "range"; x0: number; x1: number }
  | { t: "polygon"; pts: Pt[]; hover: Pt | null };

interface Edit {
  id: string;
  m: Mapping;
  orig: GateShape;
  start: Pt;
  startV: VShape;
  v: VShape;
  handle: string | null;
  changed: boolean;
}

let g: GraphCtx;
let draft: Draft | null = null;
let edit: Edit | null = null;
let busy = false;

const HANDLE_R = 7;

export function initGating(ctx: GraphCtx): void {
  g = ctx;
  const c = ctx.overlay;
  c.addEventListener("pointerdown", onDown);
  c.addEventListener("pointermove", onMove);
  c.addEventListener("pointerup", onUp);
  c.addEventListener("dblclick", onDbl);
  c.addEventListener("contextmenu", onContext);
  c.addEventListener("pointerleave", () => {
    if (draft?.t === "polygon") (draft.hover = null), drawOverlay();
  });
}

// ---------------------------------------------------------------- geometry helpers

function evPt(e: MouseEvent): { p: Pt; px: number; py: number } {
  const r = g.overlay.getBoundingClientRect();
  const px = e.clientX - r.left, py = e.clientY - r.top;
  const f = g.frame();
  return { p: [fromPx(f, px), fromPy(f, py)], px, py };
}

const inPlot = (f: Frame, px: number, py: number) => px >= f.left - 4 && px <= f.left + f.w + 4 && py >= f.top - 4 && py <= f.top + f.h + 4;

interface Visible { node: GateNode; v: VShape; m: Mapping }

function visibleGates(axes: ViewAxes, parent: string | null): Visible[] {
  const out: Visible[] = [];
  for (const node of children(parent)) {
    const r = toView(node.gate.shape, axes);
    if (r) out.push({ node, v: edit?.id === node.id ? edit.v : r.v, m: r.m });
  }
  return out;
}

function hitGate(p: Pt): Visible | null {
  const axes = g.axes();
  if (!axes) return null;
  const vis = visibleGates(axes, state.gateId);
  for (let i = vis.length - 1; i >= 0; i--) if (contains(vis[i].v, p)) return vis[i];
  return null;
}

function hitHandle(px: number, py: number): { vis: Visible; id: string } | null {
  const axes = g.axes();
  if (!axes || !state.plotGate) return null;
  const vis = visibleGates(axes, state.gateId).find((x) => x.node.id === state.plotGate);
  if (!vis) return null;
  const f = g.frame();
  for (const hd of handles(vis.v)) if (Math.hypot(toPx(f, hd.p[0]) - px, toPy(f, hd.p[1]) - py) <= HANDLE_R) return { vis, id: hd.id };
  return null;
}

// ---------------------------------------------------------------- drawing

function path(ctx: CanvasRenderingContext2D, f: Frame, pts: Pt[], close = true): void {
  ctx.beginPath();
  pts.forEach((p, i) => (i ? ctx.lineTo(toPx(f, p[0]), toPy(f, p[1])) : ctx.moveTo(toPx(f, p[0]), toPy(f, p[1]))));
  if (close) ctx.closePath();
}

function label(ctx: CanvasRenderingContext2D, f: Frame, v: VShape, text: string, color: string, compact: boolean): void {
  let [ax, ay] = labelAnchor(v);
  if (v.t === "rect" && v.min[1] === null && v.max[1] === null) ay = 0.93;
  const x = toPx(f, clampD(ax)), y = toPy(f, clampD(ay));
  ctx.font = `${compact ? 9 : 11}px -apple-system, BlinkMacSystemFont, sans-serif`;
  const w = ctx.measureText(text).width + 8;
  const hgt = compact ? 13 : 16;
  const lx = Math.min(Math.max(x - w / 2, f.left + 2), f.left + f.w - w - 2);
  const ly = Math.min(Math.max(y - hgt / 2, f.top + 2), f.top + f.h - hgt - 2);
  ctx.fillStyle = "rgba(0,0,0,0.62)";
  ctx.beginPath();
  if (typeof ctx.roundRect === "function") ctx.roundRect(lx, ly, w, hgt, 3);
  else ctx.rect(lx, ly, w, hgt);
  ctx.fill();
  ctx.fillStyle = "#fff";
  ctx.textAlign = "left";
  ctx.textBaseline = "middle";
  ctx.fillText(text, lx + 4, ly + hgt / 2 + 0.5);
  ctx.strokeStyle = color;
}

/**
 * Draw the child gates of `parent` that match the current axes (plus the parent's own
 * outline, faintly). Shared with the batch view.
 */
export function drawGates(
  ctx: CanvasRenderingContext2D, f: Frame, axes: ViewAxes, parent: string | null,
  stats: (id: string) => PopulationStats | null, opts: { compact?: boolean; selected?: string | null; editV?: VShape | null } = {},
): void {
  ctx.save();
  ctx.beginPath();
  ctx.rect(f.left, f.top, f.w, f.h);
  ctx.clip();
  const own = gate(parent);
  const ownView = own ? toView(own.gate.shape, axes) : null;
  if (ownView) {
    ctx.setLineDash([4, 4]);
    ctx.strokeStyle = readTheme().muted;
    ctx.lineWidth = 1;
    path(ctx, f, outline(ownView.v));
    ctx.stroke();
    ctx.setLineDash([]);
  }
  for (const node of children(parent)) {
    const r = toView(node.gate.shape, axes);
    if (!r) continue;
    const v = opts.selected === node.id && opts.editV ? opts.editV : r.v;
    const col = gateColor(node);
    const sel = opts.selected === node.id;
    path(ctx, f, outline(v));
    ctx.globalAlpha = sel ? 0.12 : 0.06;
    ctx.fillStyle = col;
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.lineWidth = sel ? 2.2 : opts.compact ? 1 : 1.5;
    ctx.strokeStyle = col;
    if (node.gate.negate) ctx.setLineDash([6, 3]);
    ctx.stroke();
    ctx.setLineDash([]);
    const st = stats(node.id);
    const txt = `${opts.compact ? "" : `${node.name} `}${st ? fmtPct(pct(st.count, st.parent_count)) + "%" : ""}`.trim();
    if (txt) label(ctx, f, v, txt, col, !!opts.compact);
  }
  ctx.restore();
}

export function drawOverlay(): void {
  if (!g) return;
  const f = g.frame();
  const ctx = g.overlay.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(f.dpr, 0, 0, f.dpr, 0, 0);
  ctx.clearRect(0, 0, f.cssW, f.cssH);
  const axes = g.axes();
  if (!axes || !g.data()) return;
  drawGates(ctx, f, axes, state.gateId, statFor, { selected: state.plotGate, editV: edit?.v ?? null });
  // Handles for the selected gate
  const sel = state.plotGate ? visibleGates(axes, state.gateId).find((x) => x.node.id === state.plotGate) : null;
  if (sel && state.tool === "select") {
    ctx.fillStyle = "#fff";
    ctx.strokeStyle = gateColor(sel.node);
    ctx.lineWidth = 1.5;
    for (const hd of handles(sel.v)) {
      ctx.beginPath();
      ctx.rect(toPx(f, hd.p[0]) - 4, toPy(f, hd.p[1]) - 4, 8, 8);
      ctx.fill();
      ctx.stroke();
    }
  }
  if (!draft) return;
  const accent = readTheme().accent;
  ctx.save();
  ctx.strokeStyle = accent;
  ctx.fillStyle = accent;
  ctx.lineWidth = 1.5;
  ctx.setLineDash([5, 3]);
  if (draft.t === "rect") path(ctx, f, outline({ t: "rect", min: [Math.min(draft.p0[0], draft.p1[0]), Math.min(draft.p0[1], draft.p1[1])], max: [Math.max(draft.p0[0], draft.p1[0]), Math.max(draft.p0[1], draft.p1[1])] }));
  else if (draft.t === "ellipse") path(ctx, f, outline(ellipseFromBox(draft.p0, draft.p1)));
  else if (draft.t === "range") path(ctx, f, outline({ t: "rect", min: [Math.min(draft.x0, draft.x1), null], max: [Math.max(draft.x0, draft.x1), null] }));
  else path(ctx, f, draft.hover ? [...draft.pts, draft.hover] : draft.pts, false);
  ctx.stroke();
  ctx.setLineDash([]);
  if (draft.t === "polygon") for (const p of draft.pts) ctx.fillRect(toPx(f, p[0]) - 2.5, toPy(f, p[1]) - 2.5, 5, 5);
  ctx.restore();
}

function ellipseFromBox(p0: Pt, p1: Pt): VShape {
  return { t: "ell", c: [(p0[0] + p1[0]) / 2, (p0[1] + p1[1]) / 2], a: Math.max(0.003, Math.abs(p1[0] - p0[0]) / 2), b: Math.max(0.003, Math.abs(p1[1] - p0[1]) / 2), ang: 0 };
}

// ---------------------------------------------------------------- finishing

async function addGate(name: string, shape: GateShape): Promise<string> {
  const r = await api.gateAdd(name, state.gateId, shape, false);
  await applyWorkspace(r.workspace);
  return r.id;
}

function finish(shape: GateShape): void {
  draft = null;
  drawOverlay();
  if (busy) return;
  busy = true;
  void run(async () => {
    const name = await promptText("New gate", "Population name", nextGateName());
    if (!name) return;
    const id = await addGate(name, shape);
    selectPopulation(id);
    status.ok(`Created ${name}`);
  }).finally(() => (busy = false));
}

function finishQuadrant(p: Pt, x: GateDim, y: GateDim): void {
  if (busy) return;
  busy = true;
  const xn = shortName(x.channel), yn = shortName(y.channel);
  const [cx, cy] = [clampD(p[0]), clampD(p[1])];
  const quads: [string, (number | null)[], (number | null)[]][] = [
    [`Q1: ${xn}+ ${yn}+`, [cx, cy], [null, null]],
    [`Q2: ${xn}- ${yn}+`, [null, cy], [cx, null]],
    [`Q3: ${xn}- ${yn}-`, [null, null], [cx, cy]],
    [`Q4: ${xn}+ ${yn}-`, [cx, null], [null, cy]],
  ];
  void run(async () => {
    for (const [name, min, max] of quads) await addGate(name, { type: "rectangle", dims: [x, y], min, max });
    status.ok("Created 4 quadrant gates");
  }).finally(() => (busy = false));
}

export function cancelDrawing(): boolean {
  const had = !!draft || !!edit || !!state.plotGate;
  draft = null;
  edit = null;
  if (state.plotGate) setPlotGate(null);
  drawOverlay();
  return had;
}

export function closePolygon(): boolean {
  if (draft?.t !== "polygon") return false;
  const axes = g.axes();
  if (draft.pts.length < 3 || !axes?.y) {
    status.info("A polygon needs at least 3 vertices");
    return true;
  }
  finish({ type: "polygon", dims: [axes.x, axes.y], vertices: draft.pts.map(([a, b]) => [clampD(a), clampD(b)]) });
  return true;
}

export function deleteSelectedGate(): boolean {
  if (!state.plotGate) return false;
  void deleteGate(state.plotGate);
  return true;
}

// ---------------------------------------------------------------- pointer events

function onDown(e: PointerEvent): void {
  if (e.button !== 0) return;
  const axes = g.axes();
  const f = g.frame();
  const { p, px, py } = evPt(e);
  if (!axes || !g.data() || !inPlot(f, px, py)) return;
  g.overlay.focus();
  const tool = state.tool;
  if (tool === "select") {
    const hh = hitHandle(px, py);
    const vis = hh?.vis ?? hitGate(p);
    if (!vis) return setPlotGate(null);
    if (state.plotGate !== vis.node.id) setPlotGate(vis.node.id);
    edit = { id: vis.node.id, m: vis.m, orig: vis.node.gate.shape, start: p, startV: vis.v, v: vis.v, handle: hh?.id ?? null, changed: false };
    g.overlay.setPointerCapture(e.pointerId);
    return;
  }
  if (tool === "rect") draft = { t: "rect", p0: p, p1: p };
  else if (tool === "ellipse") draft = { t: "ellipse", p0: p, p1: p };
  else if (tool === "range") draft = { t: "range", x0: p[0], x1: p[0] };
  else if (tool === "quadrant" && axes.y) return finishQuadrant(p, axes.x, axes.y);
  else if (tool === "polygon") {
    if (draft?.t !== "polygon") draft = { t: "polygon", pts: [p], hover: null };
    else {
      const first = draft.pts[0];
      if (draft.pts.length >= 3 && Math.hypot(toPx(f, first[0]) - px, toPy(f, first[1]) - py) < HANDLE_R) return void closePolygon();
      draft.pts.push(p);
    }
    return drawOverlay();
  }
  g.overlay.setPointerCapture(e.pointerId);
  drawOverlay();
}

function onMove(e: PointerEvent): void {
  const { p, px, py } = evPt(e);
  if (edit) {
    const dx = p[0] - edit.start[0], dy = p[1] - edit.start[1];
    if (!edit.changed && Math.hypot(dx, dy) < 0.002) return;
    edit.changed = true;
    edit.v = edit.handle ? dragHandle(edit.startV, edit.handle, p) : moveShape(edit.startV, dx, edit.m === "x" ? 0 : dy);
    if (edit.m === "y" && !edit.handle) edit.v = moveShape(edit.startV, 0, dy);
    return drawOverlay();
  }
  if (draft) {
    if (draft.t === "rect" || draft.t === "ellipse") draft.p1 = p;
    else if (draft.t === "range") draft.x1 = p[0];
    else draft.hover = p;
    return drawOverlay();
  }
  if (state.tool === "select") g.overlay.style.cursor = hitHandle(px, py) ? "pointer" : hitGate(p) ? "move" : "default";
}

function onUp(e: PointerEvent): void {
  if (g.overlay.hasPointerCapture(e.pointerId)) g.overlay.releasePointerCapture(e.pointerId);
  const axes = g.axes();
  if (edit) {
    const ed = edit;
    if (!ed.changed) return void (edit = null);
    const shape = fromView(normalize(ed.v), ed.m, ed.orig);
    void run(async () => {
      try {
        await applyWorkspace(await api.gateUpdate(ed.id, { shape }));
      } finally {
        if (edit === ed) edit = null;
        drawOverlay();
      }
    });
    return;
  }
  if (!draft || !axes) return;
  const f = g.frame();
  const tiny = (a: Pt, b: Pt) => Math.abs(toPx(f, a[0]) - toPx(f, b[0])) < 4 || Math.abs(toPy(f, a[1]) - toPy(f, b[1])) < 4;
  if (draft.t === "rect" && axes.y) {
    if (tiny(draft.p0, draft.p1)) return void cancelDrawing();
    const { p0, p1 } = draft;
    finish({ type: "rectangle", dims: [axes.x, axes.y], min: [clampD(Math.min(p0[0], p1[0])), clampD(Math.min(p0[1], p1[1]))], max: [clampD(Math.max(p0[0], p1[0])), clampD(Math.max(p0[1], p1[1]))] });
  } else if (draft.t === "ellipse" && axes.y) {
    if (tiny(draft.p0, draft.p1)) return void cancelDrawing();
    const v = ellipseFromBox(draft.p0, draft.p1) as Extract<VShape, { t: "ell" }>;
    finish({ type: "ellipse", dims: [axes.x, axes.y], center: v.c, a: v.a, b: v.b, angle: 0 });
  } else if (draft.t === "range") {
    if (Math.abs(toPx(f, draft.x0) - toPx(f, draft.x1)) < 3) return void cancelDrawing();
    finish({ type: "rectangle", dims: [axes.x], min: [clampD(Math.min(draft.x0, draft.x1))], max: [clampD(Math.max(draft.x0, draft.x1))] });
  }
}

function onDbl(e: MouseEvent): void {
  if (draft?.t === "polygon") {
    // The two clicks of the double-click each added a vertex; drop near-duplicates.
    const f = g.frame();
    const pts = draft.pts;
    while (pts.length > 1) {
      const a = pts[pts.length - 1], b = pts[pts.length - 2];
      if (Math.hypot(toPx(f, a[0]) - toPx(f, b[0]), toPy(f, a[1]) - toPy(f, b[1])) < 4) pts.pop();
      else break;
    }
    closePolygon();
    return;
  }
  if (state.tool === "select") {
    const vis = hitGate(evPt(e).p);
    if (vis) selectPopulation(vis.node.id);
  }
}

function onContext(e: MouseEvent): void {
  e.preventDefault();
  if (draft) return void cancelDrawing();
  const vis = hitGate(evPt(e).p);
  if (!vis) {
    if (state.gateId) showMenu(e.clientX, e.clientY, [{ label: "Back to parent population", action: () => selectPopulation(gate(state.gateId)?.parent ?? null) }]);
    return;
  }
  const id = vis.node.id;
  setPlotGate(id);
  showMenu(e.clientX, e.clientY, [
    { label: `Show “${vis.node.name}”`, action: () => selectPopulation(id) },
    { label: "Rename…", action: () => renameGate(id) },
    { label: "Change colour…", action: () => chooseColor(id, g.overlay) },
    { label: "Negate (NOT)", checked: vis.node.gate.negate, action: () => toggleNegate(id) },
    { sep: true, label: "" },
    { label: "Delete", danger: true, action: () => deleteGate(id) },
  ]);
}
