// Canvas renderers: axes, pseudo-colour dots, density map, contours, histogram.
import type { HistResult, Plot2D, Tick } from "../types";
import { colormap, lutCss } from "./colormap";
import { marchingSquares, probabilityLevels, smooth } from "./contour";
import { FULL, type Frame, type Range, toPx, toPy } from "./scale";

/** Display-space window covered by a 2-D plot's grid and points. */
export const plotRanges = (p: Plot2D): [Range, Range] => [p.header.x_range ?? FULL, p.header.y_range ?? FULL];
const inside = (r: Range, v: number) => v >= r[0] - 1e-6 && v <= r[1] + 1e-6;

export type PlotKind = "pseudo" | "density" | "contour";
export type PlotData = { kind: "2d"; plot: Plot2D } | { kind: "hist"; hist: HistResult };

export interface Theme {
  dark: boolean;
  bg: string;
  plotBg: string;
  fg: string;
  muted: string;
  axis: string;
  accent: string;
  lut: Uint8Array;
}

export function readTheme(): Theme {
  const cs = getComputedStyle(document.documentElement);
  const v = (n: string, fb: string) => cs.getPropertyValue(n).trim() || fb;
  const eff = document.documentElement.dataset.effectiveTheme;
  const dark = eff === "dark" || eff === "dim";
  return {
    dark,
    bg: v("--bg", dark ? "#1e1e1e" : "#fff"),
    plotBg: v("--plot-bg", dark ? "#151515" : "#fff"),
    fg: v("--fg", dark ? "#eee" : "#222"),
    muted: v("--muted", "#888"),
    axis: v("--axis", "#888"),
    accent: v("--accent", "#2563eb"),
    lut: colormap(dark),
  };
}

let scratch: HTMLCanvasElement | null = null;
function scratchCanvas(w: number, h: number): HTMLCanvasElement {
  scratch ??= document.createElement("canvas");
  scratch.width = w;
  scratch.height = h;
  return scratch;
}

// ---------------------------------------------------------------- axes

const SUPMAP: Record<string, string> = {
  "⁰": "0", "¹": "1", "²": "2", "³": "3", "⁴": "4", "⁵": "5",
  "⁶": "6", "⁷": "7", "⁸": "8", "⁹": "9", "⁻": "-",
};

/** Split "10^3" / "10³" / "-10²" into base + exponent for typographic rendering. */
export function splitPower(label: string): [string, string] | null {
  const m = /^(-?\d+(?:\.\d+)?)\^(-?\d+)$/.exec(label);
  if (m) return [m[1], m[2]];
  const u = /^(-?\d+)([⁰¹²³⁴-⁹⁻]+)$/.exec(label);
  if (u) return [u[1], u[2].split("").map((c) => SUPMAP[c] ?? c).join("")];
  return null;
}

function tickLabel(ctx: CanvasRenderingContext2D, label: string, x: number, y: number, align: "center" | "right"): void {
  const p = splitPower(label);
  if (!p) {
    ctx.textAlign = align;
    ctx.fillText(label, x, y);
    return;
  }
  const font = ctx.font;
  const bw = ctx.measureText(p[0]).width;
  ctx.font = font.replace(/(\d+(?:\.\d+)?)px/, (_, s) => `${Math.round(Number(s) * 0.75)}px`);
  const ew = ctx.measureText(p[1]).width;
  const total = bw + ew;
  const x0 = align === "center" ? x - total / 2 : x - total;
  ctx.textAlign = "left";
  ctx.fillText(p[1], x0 + bw, y - 4);
  ctx.font = font;
  ctx.fillText(p[0], x0, y);
}

export function drawAxes(
  ctx: CanvasRenderingContext2D, f: Frame, th: Theme,
  xTicks: Tick[], yTicks: Tick[], xLabel: string, yLabel: string,
): void {
  ctx.save();
  ctx.strokeStyle = th.axis;
  ctx.lineWidth = 1;
  ctx.strokeRect(f.left + 0.5, f.top + 0.5, f.w - 1, f.h - 1);
  if (f.compact) {
    ctx.restore();
    return;
  }
  ctx.fillStyle = th.fg;
  ctx.font = "11px -apple-system, BlinkMacSystemFont, 'Helvetica Neue', sans-serif";
  ctx.textBaseline = "top";
  ctx.beginPath();
  for (const t of xTicks) {
    if (!inside(f.xr, t.pos)) continue;
    const x = Math.round(toPx(f, t.pos)) + 0.5;
    const len = t.major ? 6 : 3;
    ctx.moveTo(x, f.top + f.h);
    ctx.lineTo(x, f.top + f.h + len);
    if (t.major && t.label) tickLabel(ctx, t.label, x, f.top + f.h + 9, "center");
  }
  ctx.textBaseline = "middle";
  for (const t of yTicks) {
    if (!inside(f.yr, t.pos)) continue;
    const y = Math.round(toPy(f, t.pos)) + 0.5;
    const len = t.major ? 6 : 3;
    ctx.moveTo(f.left, y);
    ctx.lineTo(f.left - len, y);
    if (t.major && t.label) tickLabel(ctx, t.label, f.left - 9, y, "right");
  }
  ctx.stroke();
  ctx.font = "12px -apple-system, BlinkMacSystemFont, 'Helvetica Neue', sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "bottom";
  ctx.fillText(xLabel, f.left + f.w / 2, f.cssH - 6);
  ctx.translate(14, f.top + f.h / 2);
  ctx.rotate(-Math.PI / 2);
  ctx.textBaseline = "middle";
  ctx.fillText(yLabel, 0, 0);
  ctx.restore();
}

// ---------------------------------------------------------------- 2-D

function drawPoints(ctx: CanvasRenderingContext2D, f: Frame, p: Plot2D, th: Theme, single?: string): void {
  const dpr = f.dpr;
  const pw = Math.max(1, Math.round(f.w * dpr));
  const ph = Math.max(1, Math.round(f.h * dpr));
  const c = scratchCanvas(pw, ph);
  const sctx = c.getContext("2d");
  if (!sctx) return;
  const img = sctx.createImageData(pw, ph);
  const buf = img.data;
  // Counting sort by density so dense events are painted last (on top).
  const hist = new Uint32Array(257);
  for (let i = 0; i < p.n; i++) hist[p.d[i] + 1]++;
  for (let i = 1; i < 257; i++) hist[i] += hist[i - 1];
  const order = new Uint32Array(p.n);
  for (let i = 0; i < p.n; i++) order[hist[p.d[i]]++] = i;
  const size = f.compact ? 1 : Math.max(1, Math.round(dpr));
  let sr = 0, sg = 0, sb = 0;
  if (single) {
    const m = /#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})/i.exec(single);
    if (m) [sr, sg, sb] = [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)];
  }
  const lut = th.lut;
  const xs = f.xr[1] - f.xr[0], ys = f.yr[1] - f.yr[0];
  for (let k = 0; k < p.n; k++) {
    const i = order[k];
    const px = Math.floor(((p.x[i] - f.xr[0]) / xs) * pw);
    const py = Math.floor((1 - (p.y[i] - f.yr[0]) / ys) * ph);
    const d = p.d[i] * 3;
    const r = single ? sr : lut[d], g = single ? sg : lut[d + 1], b = single ? sb : lut[d + 2];
    for (let dy = 0; dy < size; dy++) {
      const yy = py + dy;
      if (yy < 0 || yy >= ph) continue;
      for (let dx = 0; dx < size; dx++) {
        const xx = px + dx;
        if (xx < 0 || xx >= pw) continue;
        const o = (yy * pw + xx) * 4;
        buf[o] = r;
        buf[o + 1] = g;
        buf[o + 2] = b;
        buf[o + 3] = 255;
      }
    }
  }
  sctx.putImageData(img, 0, 0);
  ctx.drawImage(c, 0, 0, pw, ph, f.left, f.top, f.w, f.h);
}

function drawDensity(ctx: CanvasRenderingContext2D, f: Frame, p: Plot2D, th: Theme): void {
  const bins = p.bins;
  const c = scratchCanvas(bins, bins);
  const sctx = c.getContext("2d");
  if (!sctx) return;
  const img = sctx.createImageData(bins, bins);
  // Light smoothing so sparse regions read as a density surface rather than noise.
  const grid = smooth(p.counts, bins, bins >= 200 ? 1 : 0.6);
  let max = 0;
  for (let i = 0; i < grid.length; i++) if (grid[i] > max) max = grid[i];
  const lmax = Math.log1p(max) || 1;
  const floor = max * 1e-3;
  for (let y = 0; y < bins; y++)
    for (let x = 0; x < bins; x++) {
      const v = grid[y * bins + x];
      if (v <= floor) continue;
      const li = Math.round((255 * Math.log1p(v)) / lmax) * 3;
      const o = ((bins - 1 - y) * bins + x) * 4;
      img.data[o] = th.lut[li];
      img.data[o + 1] = th.lut[li + 1];
      img.data[o + 2] = th.lut[li + 2];
      img.data[o + 3] = 255;
    }
  sctx.putImageData(img, 0, 0);
  ctx.save();
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  const [xr, yr] = plotRanges(p);
  const x0 = toPx(f, xr[0]), x1 = toPx(f, xr[1]), y0 = toPy(f, yr[1]), y1 = toPy(f, yr[0]);
  ctx.drawImage(c, 0, 0, bins, bins, x0, y0, x1 - x0, y1 - y0);
  ctx.restore();
}

const CONTOUR_FRACTIONS = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.95, 0.98];

function drawContour(ctx: CanvasRenderingContext2D, f: Frame, p: Plot2D, th: Theme): void {
  const bins = p.bins;
  const grid = smooth(p.counts, bins, bins >= 200 ? 2 : 1.2);
  const levels = probabilityLevels(grid, CONTOUR_FRACTIONS);
  const outer = levels[0] ?? Infinity;
  // Outliers: events outside the outermost contour, drawn as faint dots.
  ctx.fillStyle = th.muted;
  const [xr, yr] = plotRanges(p);
  const cx = (xr[1] - xr[0]) / bins, cy = (yr[1] - yr[0]) / bins;
  for (let i = 0; i < p.n; i++) {
    const bx = Math.min(bins - 1, Math.max(0, Math.floor((p.x[i] - xr[0]) / cx)));
    const by = Math.min(bins - 1, Math.max(0, Math.floor((p.y[i] - yr[0]) / cy)));
    if (grid[by * bins + bx] < outer) ctx.fillRect(toPx(f, p.x[i]) - 0.5, toPy(f, p.y[i]) - 0.5, 1, 1);
  }
  const gx = (i: number) => toPx(f, xr[0] + (i + 0.5) * cx);
  const gy = (j: number) => toPy(f, yr[0] + (j + 0.5) * cy);
  ctx.lineWidth = f.compact ? 0.8 : 1.1;
  levels.forEach((lv, k) => {
    const seg = marchingSquares(grid, bins, lv);
    ctx.strokeStyle = lutCss(th.lut, (k / Math.max(1, levels.length - 1)) * 255);
    ctx.beginPath();
    for (let i = 0; i < seg.length; i += 4) {
      ctx.moveTo(gx(seg[i]), gy(seg[i + 1]));
      ctx.lineTo(gx(seg[i + 2]), gy(seg[i + 3]));
    }
    ctx.stroke();
  });
}

// ---------------------------------------------------------------- histogram

export function niceCountTicks(max: number): Tick[] {
  if (max <= 0) return [{ pos: 0, label: "0", major: true }];
  const raw = max / 4;
  const p = Math.pow(10, Math.floor(Math.log10(raw)));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * p).find((s) => s >= raw) ?? raw;
  const out: Tick[] = [];
  for (let v = 0; v <= max; v += step) out.push({ pos: v, label: v >= 1000 ? `${+(v / 1000).toFixed(1)}K` : String(v), major: true });
  return out;
}

/** Count-axis upper bound (counts are mapped linearly to display-space height). */
export function histYMax(h: HistResult): number {
  return Math.max(1, h.max_count * 1.08);
}

/** Count ticks mapped onto the frame's (linear) y range. */
export const countTicks = (f: Frame, ymax: number): Tick[] =>
  niceCountTicks(ymax).map((t) => ({ ...t, pos: f.yr[0] + (t.pos / ymax) * (f.yr[1] - f.yr[0]) }));

function histPath(ctx: CanvasRenderingContext2D, f: Frame, h: HistResult, ymax: number): void {
  const [lo, hi] = h.range;
  const bx = (i: number) => toPx(f, lo + ((hi - lo) * i) / h.bins);
  const by = (c: number) => f.top + f.h - (c / ymax) * f.h;
  ctx.beginPath();
  ctx.moveTo(bx(0), by(0));
  h.counts.forEach((c, i) => {
    ctx.lineTo(bx(i), by(c));
    ctx.lineTo(bx(i + 1), by(c));
  });
  ctx.lineTo(bx(h.bins), by(0));
  ctx.closePath();
}

function drawHist(ctx: CanvasRenderingContext2D, f: Frame, h: HistResult, th: Theme, backdrop?: HistResult): void {
  const ymax = Math.max(histYMax(h), backdrop ? histYMax(backdrop) : 0);
  ctx.save();
  ctx.beginPath();
  ctx.rect(f.left, f.top, f.w, f.h);
  ctx.clip();
  if (backdrop) {
    histPath(ctx, f, backdrop, ymax);
    ctx.fillStyle = th.muted;
    ctx.globalAlpha = 0.16;
    ctx.fill();
    ctx.globalAlpha = 0.6;
    ctx.strokeStyle = th.muted;
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.globalAlpha = 1;
  }
  histPath(ctx, f, h, ymax);
  ctx.fillStyle = lutCss(th.lut, 70, th.dark ? 0.45 : 0.3);
  ctx.fill();
  ctx.strokeStyle = lutCss(th.lut, 40);
  ctx.lineWidth = 1.2;
  ctx.stroke();
  ctx.restore();
  drawAxes(ctx, f, th, h.ticks, countTicks(f, ymax), h.label, "Count");
}

// ---------------------------------------------------------------- entry

export interface RenderOptions {
  kind: PlotKind;
  /** Override colour (used for single-colour dot plots). */
  color?: string;
  /** Drawn faintly behind the data (the parent population's events, "Show parent events"). */
  backdrop?: PlotData | null;
}

export function renderPlot(ctx: CanvasRenderingContext2D, f: Frame, data: PlotData | null, opt: RenderOptions, th: Theme, message?: string): void {
  ctx.clearRect(0, 0, f.cssW, f.cssH);
  ctx.fillStyle = th.plotBg;
  ctx.fillRect(f.left, f.top, f.w, f.h);
  if (!data) {
    drawAxes(ctx, f, th, [], [], "", "");
    if (message) {
      ctx.fillStyle = th.muted;
      ctx.font = `${f.compact ? 11 : 13}px -apple-system, BlinkMacSystemFont, sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(message, f.left + f.w / 2, f.top + f.h / 2);
    }
    return;
  }
  ctx.save();
  ctx.beginPath();
  ctx.rect(f.left, f.top, f.w, f.h);
  ctx.clip();
  if (data.kind === "hist") {
    ctx.restore();
    drawHist(ctx, f, data.hist, th, opt.backdrop?.kind === "hist" ? opt.backdrop.hist : undefined);
    return;
  }
  const p = data.plot;
  if (opt.backdrop?.kind === "2d") {
    ctx.globalAlpha = th.dark ? 0.4 : 0.3;
    drawPoints(ctx, f, opt.backdrop.plot, th, th.muted);
    ctx.globalAlpha = 1;
  }
  if (opt.kind === "density") drawDensity(ctx, f, p, th);
  else if (opt.kind === "contour") drawContour(ctx, f, p, th);
  else drawPoints(ctx, f, p, th, opt.color);
  ctx.restore();
  drawAxes(ctx, f, th, p.header.x_ticks, p.header.y_ticks, p.header.x_label, p.header.y_label);
}
