// Backend-agnostic gate drawing primitives (display space), rendered to a canvas
// here and serialised to SVG by plot/svg.ts.
import type { Pt } from "./gates";
import { clampD, type Frame, toPx, toPy } from "./scale";

export type Prim =
  | { k: "shape"; pts: Pt[]; color: string; fill: number; width: number; dash: number[]; stroke: boolean }
  | { k: "line"; a: Pt; b: Pt; color: string; width: number }
  | { k: "dot"; at: Pt; color: string; r: number }
  /** `dir` = direction the label box grows from the anchor (view space, y up); [0,0] = centred. */
  | { k: "label"; at: Pt; text: string; color: string; dir: [number, number] };

export const LABEL_FONT = "-apple-system, BlinkMacSystemFont, 'Helvetica Neue', sans-serif";
export const labelSize = (compact: boolean) => ({ font: compact ? 9 : 11, h: compact ? 13 : 16 });

/** Pixel box of a label, clamped inside the plot area. `width` = measured text width. */
export function labelBox(f: Frame, p: Extract<Prim, { k: "label" }>, width: number, compact: boolean): { x: number; y: number; w: number; h: number } {
  const w = width + 8;
  const hgt = labelSize(compact).h;
  const inset = p.dir[0] || p.dir[1] ? 3 : 0;
  const x = toPx(f, clampD(p.at[0])) + p.dir[0] * inset, y = toPy(f, clampD(p.at[1])) - p.dir[1] * inset;
  let lx = x - (w * (1 - p.dir[0])) / 2;
  let ly = y - (hgt * (1 + p.dir[1])) / 2;
  lx = Math.min(Math.max(lx, f.left + 2), f.left + f.w - w - 2);
  ly = Math.min(Math.max(ly, f.top + 2), f.top + f.h - hgt - 2);
  return { x: lx, y: ly, w, h: hgt };
}

function pathPts(ctx: CanvasRenderingContext2D, f: Frame, pts: Pt[]): void {
  ctx.beginPath();
  pts.forEach((p, i) => (i ? ctx.lineTo(toPx(f, p[0]), toPy(f, p[1])) : ctx.moveTo(toPx(f, p[0]), toPy(f, p[1]))));
  ctx.closePath();
}

export function drawPrims(ctx: CanvasRenderingContext2D, f: Frame, prims: Prim[], compact: boolean): void {
  ctx.save();
  ctx.beginPath();
  ctx.rect(f.left, f.top, f.w, f.h);
  ctx.clip();
  for (const p of prims) {
    if (p.k === "shape") {
      pathPts(ctx, f, p.pts);
      if (p.fill > 0) {
        ctx.globalAlpha = p.fill;
        ctx.fillStyle = p.color;
        ctx.fill();
        ctx.globalAlpha = 1;
      }
      if (p.stroke) {
        ctx.lineWidth = p.width;
        ctx.strokeStyle = p.color;
        ctx.setLineDash(p.dash);
        ctx.stroke();
        ctx.setLineDash([]);
      }
    } else if (p.k === "line") {
      ctx.beginPath();
      ctx.moveTo(toPx(f, p.a[0]), toPy(f, p.a[1]));
      ctx.lineTo(toPx(f, p.b[0]), toPy(f, p.b[1]));
      ctx.strokeStyle = p.color;
      ctx.lineWidth = p.width;
      ctx.stroke();
    } else if (p.k === "dot") {
      ctx.beginPath();
      ctx.arc(toPx(f, p.at[0]), toPy(f, p.at[1]), p.r, 0, Math.PI * 2);
      ctx.fillStyle = p.color;
      ctx.fill();
    } else {
      const sz = labelSize(compact);
      ctx.font = `${sz.font}px ${LABEL_FONT}`;
      const b = labelBox(f, p, ctx.measureText(p.text).width, compact);
      ctx.fillStyle = "rgba(0,0,0,0.62)";
      ctx.beginPath();
      if (typeof ctx.roundRect === "function") ctx.roundRect(b.x, b.y, b.w, b.h, 3);
      else ctx.rect(b.x, b.y, b.w, b.h);
      ctx.fill();
      ctx.fillStyle = p.color;
      ctx.fillRect(b.x, b.y + 2, 2, b.h - 4);
      ctx.fillStyle = "#fff";
      ctx.textAlign = "left";
      ctx.textBaseline = "middle";
      ctx.fillText(p.text, b.x + 4, b.y + b.h / 2 + 0.5);
    }
  }
  ctx.restore();
}
