// Layout panel rendering (screen, PNG page export) and SVG page serialisation.
import { colormap } from "../plot/colormap";
import type { ViewAxes } from "../plot/gates";
import type { Prim } from "../plot/prims";
import { renderPlot, type Theme } from "../plot/render";
import { makeFrame, type Frame } from "../plot/scale";
import { panelSvg, svgDocument } from "../plot/svg";
import { gate, gatePath, shortName } from "../state";
import type { PopulationStats } from "../types";
import { fmtInt, fmtPct, pct } from "./dom";
import { drawGates, gatePrims, type StatLookup } from "./gatedraw";
import { CAPTION_H, type Layout, type Panel, type PanelData, pageDims, TITLE_H } from "./layoutmodel";

/** Pages are paper: always rendered with a light theme regardless of the app theme. */
export const PRINT_THEME: Theme = {
  dark: false, bg: "#ffffff", plotBg: "#ffffff", fg: "#1d1d1f", muted: "#8a8a8f", axis: "#6b6b70", accent: "#2463eb", lut: colormap(false),
};
const GATE_OPTS = { lineColor: "#2b2b2e", mutedColor: "#9a9aa0" };
export const FONT = "-apple-system, BlinkMacSystemFont, 'Helvetica Neue', Helvetica, Arial, sans-serif";

const lookup = (stats: PopulationStats[] | null): StatLookup => (id) => stats?.find((r) => r.gate_id === id) ?? null;
const bodyH = (p: Panel) => Math.max(20, p.h - TITLE_H - CAPTION_H);
const compactFor = (p: Panel) => Math.min(p.w, bodyH(p)) < 170;

export function panelTitle(p: Panel): string {
  if (p.title.trim()) return p.title.trim();
  if (p.kind === "stats") return "Population statistics";
  const x = p.xDim ? shortName(p.xDim.channel) : "";
  return p.yDim ? `${x} × ${shortName(p.yDim.channel)}` : `${x} histogram`;
}

export function panelCaption(p: Panel, d: PanelData | null): [string, string] {
  const path = p.gateId && gate(p.gateId) ? gatePath(p.gateId) : p.gatePath || "All events";
  const st = d?.stats?.find((r) => r.gate_id === p.gateId) ?? null;
  if (p.kind === "stats") return [p.sampleName, st ? `${fmtInt(st.total)} events` : ""];
  const num = st ? (p.gateId ? `${fmtPct(pct(st.count, st.parent_count))}% of parent · ${fmtInt(st.count)}` : `${fmtInt(st.count)} events`) : "";
  return [path, [p.sampleName, num].filter(Boolean).join("  ·  ")];
}

export const panelFrame = (p: Panel, dpr: number): Frame => makeFrame(p.w, bodyH(p), dpr, compactFor(p));
const panelAxes = (p: Panel): ViewAxes | null => (p.xDim ? { x: p.xDim, y: p.yDim } : null);

// ---------------------------------------------------------------- stats table

interface Row { name: string; depth: number; count: string; parent: string; total: string }

function statRows(stats: PopulationStats[]): Row[] {
  return stats.map((s) => {
    let depth = 0;
    for (let g = gate(s.gate_id); g?.parent; g = gate(g.parent)) depth++;
    return {
      name: s.name, depth: s.gate_id ? depth + 1 : 0, count: fmtInt(s.count),
      parent: s.gate_id ? fmtPct(s.pct_parent) : "", total: fmtPct(s.pct_total),
    };
  });
}

const STAT_COLS = [{ t: "Count", w: 0.2 }, { t: "% Parent", w: 0.16 }, { t: "% Total", w: 0.16 }];
const ROW_H = 15;

function drawStats(ctx: CanvasRenderingContext2D, w: number, hgt: number, stats: PopulationStats[] | null): void {
  ctx.fillStyle = PRINT_THEME.plotBg;
  ctx.fillRect(0, 0, w, hgt);
  if (!stats) return;
  ctx.font = `10px ${FONT}`;
  ctx.textBaseline = "middle";
  const right = [w - 4, w - 4 - STAT_COLS[2].w * w, w - 4 - (STAT_COLS[2].w + STAT_COLS[1].w) * w];
  const head = (y: number) => {
    ctx.fillStyle = PRINT_THEME.muted;
    ctx.textAlign = "left";
    ctx.fillText("Population", 4, y);
    ctx.textAlign = "right";
    STAT_COLS.forEach((c, i) => ctx.fillText(c.t, right[2 - i], y));
  };
  head(ROW_H / 2);
  statRows(stats).forEach((r, i) => {
    const y = ROW_H * (i + 1.5);
    if (y > hgt - 4) return;
    ctx.fillStyle = i % 2 ? "#ffffff" : "#f4f4f5";
    ctx.fillRect(0, y - ROW_H / 2, w, ROW_H);
    ctx.fillStyle = PRINT_THEME.fg;
    ctx.textAlign = "left";
    ctx.fillText(r.name, 4 + r.depth * 8, y, Math.max(20, right[2] - STAT_COLS[0].w * w - 8 - r.depth * 8));
    ctx.textAlign = "right";
    ctx.fillText(r.count, right[2], y);
    ctx.fillText(r.parent, right[1], y);
    ctx.fillText(r.total, right[0], y);
  });
}

// ---------------------------------------------------------------- canvas

/** Render a panel's body (plot or table) into `ctx`, whose transform maps CSS px -> device px. */
export function renderPanelBody(ctx: CanvasRenderingContext2D, p: Panel, d: PanelData | null, dpr: number): void {
  if (p.kind === "stats") return drawStats(ctx, p.w, bodyH(p), d?.stats ?? null);
  const f = panelFrame(p, dpr);
  renderPlot(ctx, f, d?.plot ?? null, { kind: p.plotKind }, PRINT_THEME, d ? d.message ?? (d.plot ? "" : "No data") : "Loading…");
  const axes = panelAxes(p);
  if (d?.plot && axes) drawGates(ctx, f, axes, p.gateId, lookup(d.stats), { compact: f.compact, ...GATE_OPTS });
}

/** Draw a whole panel (title, body, caption) at its page position; ctx is in page CSS px. */
export function drawPanelOnPage(ctx: CanvasRenderingContext2D, p: Panel, d: PanelData | null, dpr: number): void {
  const bh = bodyH(p);
  const tmp = document.createElement("canvas");
  tmp.width = Math.max(1, Math.round(p.w * dpr));
  tmp.height = Math.max(1, Math.round(bh * dpr));
  const tctx = tmp.getContext("2d");
  if (tctx) {
    tctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    tctx.fillStyle = "#fff";
    tctx.fillRect(0, 0, p.w, bh);
    renderPanelBody(tctx, p, d, dpr);
  }
  ctx.save();
  ctx.translate(p.x, p.y);
  ctx.fillStyle = PRINT_THEME.fg;
  ctx.font = `600 12px ${FONT}`;
  ctx.textBaseline = "middle";
  ctx.textAlign = "left";
  ctx.fillText(panelTitle(p), 2, TITLE_H / 2, p.w - 4);
  ctx.drawImage(tmp, 0, 0, tmp.width, tmp.height, 0, TITLE_H, p.w, bh);
  const [c1, c2] = panelCaption(p, d);
  ctx.font = `10px ${FONT}`;
  ctx.fillStyle = "#4a4a4f";
  ctx.fillText(c1, 2, TITLE_H + bh + 8, p.w - 4);
  ctx.fillStyle = PRINT_THEME.muted;
  ctx.fillText(c2, 2, TITLE_H + bh + 22, p.w - 4);
  ctx.restore();
}

/** Render the page to an offscreen canvas at `scale`× and return a PNG data URL. */
export function pagePng(l: Layout, data: (p: Panel) => PanelData | null, scale = 2): string {
  const [pw, ph] = pageDims(l);
  const c = document.createElement("canvas");
  c.width = pw * scale;
  c.height = ph * scale;
  const ctx = c.getContext("2d");
  if (!ctx) throw new Error("canvas 2D context unavailable");
  ctx.setTransform(scale, 0, 0, scale, 0, 0);
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, pw, ph);
  for (const p of l.panels) drawPanelOnPage(ctx, p, data(p), scale);
  return c.toDataURL("image/png");
}

// ---------------------------------------------------------------- SVG

export function pageSvg(l: Layout, data: (p: Panel) => PanelData | null): string {
  const [pw, ph] = pageDims(l);
  const parts = l.panels.map((p) => {
    const d = data(p);
    const f = panelFrame(p, 1);
    const axes = panelAxes(p);
    const prims: Prim[] = d?.plot && axes ? gatePrims(axes, p.gateId, lookup(d.stats), { compact: f.compact, ...GATE_OPTS }) : [];
    return panelSvg({
      x: p.x, y: p.y, w: p.w, titleH: TITLE_H, bodyH: bodyH(p), title: panelTitle(p), caption: panelCaption(p, d),
      frame: f, plot: p.kind === "plot" ? d?.plot ?? null : null, kind: p.plotKind, prims, theme: PRINT_THEME,
      table: p.kind === "stats" && d?.stats ? statRows(d.stats).map((r) => [`${"  ".repeat(r.depth)}${r.name}`, r.count, r.parent, r.total]) : null,
      message: d?.message,
    });
  });
  return svgDocument(pw, ph, parts.join("\n"));
}
