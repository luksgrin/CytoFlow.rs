// SVG serialisation of layout panels: axes, ticks, gate outlines, data marks
// (points as <circle>s subsampled to <= 5000, density as <rect>s, contours as paths).
import type { Tick } from "../types";
import { lutCss } from "./colormap";
import { marchingSquares, probabilityLevels, smooth } from "./contour";
import { labelBox, labelSize, LABEL_FONT, type Prim } from "./prims";
import { histYMax, niceCountTicks, type PlotData, type PlotKind, splitPower, type Theme } from "./render";
import { D0, DSPAN, type Frame, toPx, toPy } from "./scale";

export const MAX_SVG_POINTS = 5000;
const FONT = "-apple-system, 'Helvetica Neue', Helvetica, Arial, sans-serif";

export interface PanelSvgInput {
  x: number;
  y: number;
  w: number;
  titleH: number;
  bodyH: number;
  title: string;
  caption: [string, string];
  frame: Frame;
  plot: PlotData | null;
  kind: PlotKind;
  prims: Prim[];
  theme: Theme;
  table: string[][] | null; // [name, count, %parent, %total]
  message?: string;
}

export const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const n2 = (v: number) => (Math.round(v * 100) / 100).toString();

function text(x: number, y: number, s: string, attrs: string): string {
  return `<text x="${n2(x)}" y="${n2(y)}" ${attrs}>${esc(s)}</text>`;
}

function tickText(x: number, y: number, label: string, anchor: string): string {
  const p = splitPower(label);
  const body = p ? `${esc(p[0])}<tspan dy="-4" font-size="8">${esc(p[1])}</tspan>` : esc(label);
  return `<text x="${n2(x)}" y="${n2(y)}" text-anchor="${anchor}" font-size="10" fill="currentColor">${body}</text>`;
}

function axesSvg(f: Frame, th: Theme, xt: Tick[], yt: Tick[], xl: string, yl: string): string {
  const o: string[] = [`<rect x="${n2(f.left + 0.5)}" y="${n2(f.top + 0.5)}" width="${n2(f.w - 1)}" height="${n2(f.h - 1)}" fill="none" stroke="${th.axis}" stroke-width="1"/>`];
  if (f.compact) return o.join("");
  const ticks: string[] = [];
  const labels: string[] = [];
  for (const t of xt) {
    const x = toPx(f, t.pos);
    ticks.push(`M${n2(x)} ${n2(f.top + f.h)}v${t.major ? 6 : 3}`);
    if (t.major && t.label) labels.push(tickText(x, f.top + f.h + 19, t.label, "middle"));
  }
  for (const t of yt) {
    const y = toPy(f, t.pos);
    ticks.push(`M${n2(f.left)} ${n2(y)}h${t.major ? -6 : -3}`);
    if (t.major && t.label) labels.push(tickText(f.left - 9, y + 3.5, t.label, "end"));
  }
  o.push(`<path d="${ticks.join("")}" stroke="${th.axis}" stroke-width="1" fill="none"/>`);
  o.push(`<g style="color:${th.fg}">${labels.join("")}</g>`);
  o.push(text(f.left + f.w / 2, f.cssH - 8, xl, `text-anchor="middle" font-size="11" fill="${th.fg}"`));
  o.push(`<text transform="translate(14 ${n2(f.top + f.h / 2)}) rotate(-90)" text-anchor="middle" dominant-baseline="middle" font-size="11" fill="${th.fg}">${esc(yl)}</text>`);
  return o.join("");
}

function pointsSvg(f: Frame, d: Extract<PlotData, { kind: "2d" }>["plot"], th: Theme): string {
  const stride = Math.max(1, Math.ceil(d.n / MAX_SVG_POINTS));
  const idx: number[] = [];
  for (let i = 0; i < d.n; i += stride) idx.push(i);
  idx.sort((a, b) => d.d[a] - d.d[b]);
  const r = f.compact ? 0.7 : 1;
  return idx.map((i) => `<circle cx="${n2(toPx(f, d.x[i]))}" cy="${n2(toPy(f, d.y[i]))}" r="${r}" fill="${lutCss(th.lut, d.d[i])}"/>`).join("");
}

function densitySvg(f: Frame, d: Extract<PlotData, { kind: "2d" }>["plot"], th: Theme): string {
  const k = Math.max(1, Math.ceil(d.bins / 64));
  const nb = Math.floor(d.bins / k);
  const agg = new Float32Array(nb * nb);
  for (let y = 0; y < nb * k; y++) for (let x = 0; x < nb * k; x++) agg[Math.floor(y / k) * nb + Math.floor(x / k)] += d.counts[y * d.bins + x];
  const grid = smooth(agg, nb, 0.6);
  let max = 0;
  for (const v of grid) if (v > max) max = v;
  const lmax = Math.log1p(max) || 1;
  const cw = f.w / nb, chh = f.h / nb;
  const out: string[] = [];
  for (let y = 0; y < nb; y++)
    for (let x = 0; x < nb; x++) {
      const v = grid[y * nb + x];
      if (v <= max * 1e-3) continue;
      out.push(`<rect x="${n2(f.left + x * cw)}" y="${n2(f.top + f.h - (y + 1) * chh)}" width="${n2(cw + 0.3)}" height="${n2(chh + 0.3)}" fill="${lutCss(th.lut, (255 * Math.log1p(v)) / lmax)}"/>`);
    }
  return out.join("");
}

function contourSvg(f: Frame, d: Extract<PlotData, { kind: "2d" }>["plot"], th: Theme): string {
  const grid = smooth(d.counts, d.bins, d.bins >= 200 ? 2 : 1.2);
  const levels = probabilityLevels(grid, [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.95, 0.98]);
  const cell = DSPAN / d.bins;
  const gx = (i: number) => n2(toPx(f, D0 + (i + 0.5) * cell));
  const gy = (j: number) => n2(toPy(f, D0 + (j + 0.5) * cell));
  return levels.map((lv, k) => {
    const seg = marchingSquares(grid, d.bins, lv);
    const dd: string[] = [];
    for (let i = 0; i < seg.length; i += 4) dd.push(`M${gx(seg[i])} ${gy(seg[i + 1])}L${gx(seg[i + 2])} ${gy(seg[i + 3])}`);
    return `<path d="${dd.join("")}" fill="none" stroke="${lutCss(th.lut, (k / Math.max(1, levels.length - 1)) * 255)}" stroke-width="1"/>`;
  }).join("");
}

function histSvg(f: Frame, h: Extract<PlotData, { kind: "hist" }>["hist"], th: Theme): { marks: string; yTicks: Tick[] } {
  const [lo, hi] = h.range;
  const ymax = histYMax(h);
  const bx = (i: number) => n2(toPx(f, lo + ((hi - lo) * i) / h.bins));
  const by = (c: number) => n2(f.top + f.h - (c / ymax) * f.h);
  let dd = `M${bx(0)} ${by(0)}`;
  h.counts.forEach((c, i) => (dd += `L${bx(i)} ${by(c)}L${bx(i + 1)} ${by(c)}`));
  dd += `L${bx(h.bins)} ${by(0)}Z`;
  const marks = `<path d="${dd}" fill="${lutCss(th.lut, 70, 0.3)}" stroke="${lutCss(th.lut, 40)}" stroke-width="1.2"/>`;
  return { marks, yTicks: niceCountTicks(ymax).map((t) => ({ ...t, pos: D0 + (t.pos / ymax) * DSPAN })) };
}

function primsSvg(f: Frame, prims: Prim[], compact: boolean): string {
  const P = (x: number, y: number) => `${n2(toPx(f, x))},${n2(toPy(f, y))}`;
  const sz = labelSize(compact);
  return prims.map((p) => {
    if (p.k === "shape") {
      const fill = p.fill > 0 ? `fill="${p.color}" fill-opacity="${p.fill}"` : `fill="none"`;
      const stroke = p.stroke ? `stroke="${p.color}" stroke-width="${p.width}"${p.dash.length ? ` stroke-dasharray="${p.dash.join(" ")}"` : ""}` : "";
      return `<polygon points="${p.pts.map((q) => P(q[0], q[1])).join(" ")}" ${fill} ${stroke}/>`;
    }
    if (p.k === "line") return `<line x1="${n2(toPx(f, p.a[0]))}" y1="${n2(toPy(f, p.a[1]))}" x2="${n2(toPx(f, p.b[0]))}" y2="${n2(toPy(f, p.b[1]))}" stroke="${p.color}" stroke-width="${p.width}"/>`;
    if (p.k === "dot") return `<circle cx="${n2(toPx(f, p.at[0]))}" cy="${n2(toPy(f, p.at[1]))}" r="${p.r}" fill="${p.color}"/>`;
    const b = labelBox(f, p, p.text.length * sz.font * 0.56, compact);
    return `<g><rect x="${n2(b.x)}" y="${n2(b.y)}" width="${n2(b.w)}" height="${b.h}" rx="3" fill="#000" fill-opacity="0.62"/>` +
      `<rect x="${n2(b.x)}" y="${n2(b.y + 2)}" width="2" height="${b.h - 4}" fill="${p.color}"/>` +
      text(b.x + 4, b.y + b.h / 2 + sz.font * 0.36, p.text, `font-size="${sz.font}" font-family="${LABEL_FONT.replace(/"/g, "'")}" fill="#fff"`) + `</g>`;
  }).join("");
}

function tableSvg(w: number, rows: string[][], th: Theme, maxH: number): string {
  const right = [w - 4, w - 4 - 0.16 * w, w - 4 - 0.32 * w];
  const o: string[] = [];
  const row = (y: number, cells: string[], fill: string) => {
    o.push(text(4, y, cells[0], `font-size="10" fill="${fill}" xml:space="preserve"`));
    [cells[1], cells[2], cells[3]].forEach((c, i) => o.push(text(right[2 - i], y, c, `font-size="10" text-anchor="end" fill="${fill}"`)));
  };
  row(11, ["Population", "Count", "% Parent", "% Total"], th.muted);
  rows.forEach((r, i) => {
    const y = 15 * (i + 1.5);
    if (y > maxH - 4) return;
    if (i % 2 === 0) o.push(`<rect x="0" y="${n2(y - 7.5)}" width="${n2(w)}" height="15" fill="#f4f4f5"/>`);
    row(y + 3.5, r, th.fg);
  });
  return o.join("");
}

let clipSeq = 0;

export function panelSvg(i: PanelSvgInput): string {
  const f = i.frame, th = i.theme;
  const o: string[] = [`<g transform="translate(${n2(i.x)} ${n2(i.y)})" font-family="${FONT}">`];
  o.push(text(2, i.titleH / 2 + 4, i.title, `font-size="12" font-weight="600" fill="${th.fg}"`));
  o.push(`<g transform="translate(0 ${i.titleH})">`);
  if (i.table) o.push(tableSvg(i.w, i.table, th, i.bodyH));
  else {
    const id = `cf-clip-${++clipSeq}`;
    o.push(`<clipPath id="${id}"><rect x="${n2(f.left)}" y="${n2(f.top)}" width="${n2(f.w)}" height="${n2(f.h)}"/></clipPath>`);
    o.push(`<rect x="${n2(f.left)}" y="${n2(f.top)}" width="${n2(f.w)}" height="${n2(f.h)}" fill="${th.plotBg}"/>`);
    const p = i.plot;
    let marks = "";
    let axes = axesSvg(f, th, [], [], "", "");
    if (p?.kind === "hist") {
      const hs = histSvg(f, p.hist, th);
      marks = hs.marks;
      axes = axesSvg(f, th, p.hist.ticks, hs.yTicks, p.hist.label, "Count");
    } else if (p?.kind === "2d") {
      marks = i.kind === "density" ? densitySvg(f, p.plot, th) : i.kind === "contour" ? contourSvg(f, p.plot, th) : pointsSvg(f, p.plot, th);
      const hd = p.plot.header;
      axes = axesSvg(f, th, hd.x_ticks, hd.y_ticks, hd.x_label, hd.y_label);
    } else if (i.message) {
      o.push(text(f.left + f.w / 2, f.top + f.h / 2, i.message, `text-anchor="middle" font-size="11" fill="${th.muted}"`));
    }
    o.push(`<g clip-path="url(#${id})">${marks}${primsSvg(f, i.prims, f.compact)}</g>`, axes);
  }
  o.push(`</g>`);
  o.push(text(2, i.titleH + i.bodyH + 11, i.caption[0], `font-size="10" fill="#4a4a4f"`));
  o.push(text(2, i.titleH + i.bodyH + 25, i.caption[1], `font-size="10" fill="${th.muted}"`));
  o.push(`</g>`);
  return o.join("\n");
}

export function svgDocument(w: number, h: number, body: string): string {
  return `<?xml version="1.0" encoding="UTF-8"?>\n<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">\n` +
    `<rect width="${w}" height="${h}" fill="#ffffff"/>\n${body}\n</svg>\n`;
}
