// Overlay rendering: several populations / samples on one set of axes.
// 1-D: outlined histograms (optionally normalised); 2-D: contour outlines or dot clouds.
import type { HistResult, Plot2D } from "../types";
import { marchingSquares, probabilityLevels, smooth } from "./contour";
import { countTicks, drawAxes, plotRanges, type PlotData, type Theme } from "./render";
import { type Frame, toPx, toPy } from "./scale";

export type HistNorm = "mode" | "total" | "count";
export type OverlayStyle = "contour" | "dots";

export interface Series {
  label: string;
  color: string;
  data: PlotData;
}

export interface OverlayOptions {
  norm: HistNorm;
  style: OverlayStyle;
}

export const NORM_LABEL: Record<HistNorm, string> = { mode: "% of max", total: "% of total", count: "Count" };
const LEVELS = [0.5, 0.75, 0.9, 0.97];

/** Normalised bin heights of a histogram. */
export function histValues(h: HistResult, norm: HistNorm): number[] {
  if (norm === "count") return h.counts;
  const denom = norm === "mode" ? Math.max(1, ...h.counts) : Math.max(1, h.counts.reduce((a, b) => a + b, 0));
  return h.counts.map((c) => (100 * c) / denom);
}

/** Contour segments (bin-index units) per probability level for one 2-D plot. */
export function contourSegments(p: Plot2D): Float32Array[] {
  const grid = smooth(p.counts, p.bins, p.bins >= 200 ? 2 : 1.2);
  return probabilityLevels(grid, LEVELS).map((lv) => marchingSquares(grid, p.bins, lv));
}

function drawHistSeries(ctx: CanvasRenderingContext2D, f: Frame, series: Series[], norm: HistNorm, th: Theme): void {
  const hs = series.filter((s) => s.data.kind === "hist") as (Series & { data: { kind: "hist"; hist: HistResult } })[];
  const vals = hs.map((s) => histValues(s.data.hist, norm));
  const ymax = Math.max(1e-9, ...vals.map((v) => Math.max(0, ...v))) * 1.08;
  ctx.save();
  ctx.beginPath();
  ctx.rect(f.left, f.top, f.w, f.h);
  ctx.clip();
  hs.forEach((s, k) => {
    const h = s.data.hist;
    const [lo, hi] = h.range;
    const bx = (i: number) => toPx(f, lo + ((hi - lo) * i) / h.bins);
    const by = (c: number) => f.top + f.h - (c / ymax) * f.h;
    ctx.beginPath();
    ctx.moveTo(bx(0), by(0));
    vals[k].forEach((c, i) => (ctx.lineTo(bx(i), by(c)), ctx.lineTo(bx(i + 1), by(c))));
    ctx.lineTo(bx(h.bins), by(0));
    ctx.globalAlpha = 0.1;
    ctx.fillStyle = s.color;
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.strokeStyle = s.color;
    ctx.lineWidth = f.compact ? 1 : 1.5;
    ctx.stroke();
  });
  ctx.restore();
  const yt = countTicks(f, ymax);
  const first = hs[0]?.data.hist;
  drawAxes(ctx, f, th, first?.ticks ?? [], yt, first?.label ?? "", NORM_LABEL[norm]);
}

function draw2DSeries(ctx: CanvasRenderingContext2D, f: Frame, series: Series[], style: OverlayStyle, th: Theme): void {
  const ps = series.filter((s) => s.data.kind === "2d") as (Series & { data: { kind: "2d"; plot: Plot2D } })[];
  ctx.save();
  ctx.beginPath();
  ctx.rect(f.left, f.top, f.w, f.h);
  ctx.clip();
  for (const s of ps) {
    const p = s.data.plot;
    if (style === "dots") {
      ctx.fillStyle = s.color;
      ctx.globalAlpha = ps.length > 1 ? 0.55 : 0.8;
      for (let i = 0; i < p.n; i++) ctx.fillRect(toPx(f, p.x[i]) - 0.6, toPy(f, p.y[i]) - 0.6, 1.2, 1.2);
      ctx.globalAlpha = 1;
      continue;
    }
    const [xr, yr] = plotRanges(p);
    const cx = (xr[1] - xr[0]) / p.bins, cy = (yr[1] - yr[0]) / p.bins;
    const gx = (i: number) => toPx(f, xr[0] + (i + 0.5) * cx);
    const gy = (j: number) => toPy(f, yr[0] + (j + 0.5) * cy);
    ctx.strokeStyle = s.color;
    contourSegments(p).forEach((seg, k, all) => {
      ctx.lineWidth = (f.compact ? 0.8 : 1.1) + (k === all.length - 1 ? 0.4 : 0);
      ctx.beginPath();
      for (let i = 0; i < seg.length; i += 4) {
        ctx.moveTo(gx(seg[i]), gy(seg[i + 1]));
        ctx.lineTo(gx(seg[i + 2]), gy(seg[i + 3]));
      }
      ctx.stroke();
    });
  }
  ctx.restore();
  const h = ps[0]?.data.plot.header;
  drawAxes(ctx, f, th, h?.x_ticks ?? [], h?.y_ticks ?? [], h?.x_label ?? "", h?.y_label ?? "");
}

export function drawLegend(ctx: CanvasRenderingContext2D, f: Frame, series: Series[], th: Theme): void {
  if (series.length < 2 && !series[0]?.label) return;
  ctx.save();
  const fs = f.compact ? 9 : 11;
  ctx.font = `${fs}px -apple-system, BlinkMacSystemFont, 'Helvetica Neue', sans-serif`;
  const row = fs + 5;
  const w = Math.min(f.w * 0.6, Math.max(...series.map((s) => ctx.measureText(s.label).width)) + 26);
  const hgt = row * series.length + 6;
  const x = f.left + f.w - w - 6, y = f.top + 6;
  ctx.globalAlpha = 0.88;
  ctx.fillStyle = th.plotBg;
  ctx.fillRect(x, y, w, hgt);
  ctx.globalAlpha = 1;
  ctx.strokeStyle = th.axis;
  ctx.lineWidth = 0.6;
  ctx.strokeRect(x + 0.5, y + 0.5, w - 1, hgt - 1);
  ctx.textBaseline = "middle";
  ctx.textAlign = "left";
  series.forEach((s, i) => {
    const cy = y + 3 + row * i + row / 2;
    ctx.fillStyle = s.color;
    ctx.fillRect(x + 6, cy - 1.5, 12, 3);
    ctx.fillStyle = th.fg;
    let t = s.label;
    while (t.length > 4 && ctx.measureText(t).width > w - 28) t = t.slice(0, -2);
    ctx.fillText(t === s.label ? t : `${t}…`, x + 22, cy);
  });
  ctx.restore();
}

export function renderOverlay(ctx: CanvasRenderingContext2D, f: Frame, series: Series[], opt: OverlayOptions, th: Theme): void {
  ctx.clearRect(0, 0, f.cssW, f.cssH);
  ctx.fillStyle = th.plotBg;
  ctx.fillRect(f.left, f.top, f.w, f.h);
  if (series.some((s) => s.data.kind === "hist")) drawHistSeries(ctx, f, series, opt.norm, th);
  else draw2DSeries(ctx, f, series, opt.style, th);
  drawLegend(ctx, f, series, th);
}
