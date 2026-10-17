// Display-space <-> pixel mapping. Un-zoomed, both axes cover display space
// [-0.02, 1.02]; a zoomed graph window maps a sub-range (`Frame.xr` / `yr`).
export const D0 = -0.02;
export const D1 = 1.02;
export const DSPAN = D1 - D0;

export type Range = [number, number];
export const FULL: Range = [D0, D1];
export const isFull = (r: Range) => r[0] <= D0 + 1e-9 && r[1] >= D1 - 1e-9;

export interface Frame {
  cssW: number;
  cssH: number;
  dpr: number;
  left: number; // plot area, CSS px
  top: number;
  w: number;
  h: number;
  compact: boolean;
  /** Visible display-space range of each axis. */
  xr: Range;
  yr: Range;
}

export function makeFrame(cssW: number, cssH: number, dpr: number, compact = false, xr: Range = FULL, yr: Range = FULL): Frame {
  const m = compact ? { l: 6, r: 6, t: 6, b: 6 } : { l: 62, r: 16, t: 14, b: 48 };
  return {
    cssW, cssH, dpr, compact, xr: [xr[0], xr[1]], yr: [yr[0], yr[1]],
    left: m.l, top: m.t,
    w: Math.max(10, cssW - m.l - m.r),
    h: Math.max(10, cssH - m.t - m.b),
  };
}

export const toPx = (f: Frame, v: number) => f.left + ((v - f.xr[0]) / (f.xr[1] - f.xr[0])) * f.w;
export const toPy = (f: Frame, v: number) => f.top + f.h - ((v - f.yr[0]) / (f.yr[1] - f.yr[0])) * f.h;
export const fromPx = (f: Frame, px: number) => f.xr[0] + ((px - f.left) / f.w) * (f.xr[1] - f.xr[0]);
export const fromPy = (f: Frame, py: number) => f.yr[0] + ((f.top + f.h - py) / f.h) * (f.yr[1] - f.yr[0]);
/** Same frame, another visible range. */
export const withRange = (f: Frame, xr: Range, yr: Range): Frame => ({ ...f, xr: [xr[0], xr[1]], yr: [yr[0], yr[1]] });
export const clampD = (v: number) => Math.min(D1, Math.max(D0, v));

/** Size a canvas for the device pixel ratio and return a context scaled to CSS px. */
export function setupCanvas(c: HTMLCanvasElement, cssW: number, cssH: number): CanvasRenderingContext2D {
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(cssW * dpr));
  const h = Math.max(1, Math.round(cssH * dpr));
  if (c.width !== w) c.width = w;
  if (c.height !== h) c.height = h;
  c.style.width = `${cssW}px`;
  c.style.height = `${cssH}px`;
  const ctx = c.getContext("2d");
  if (!ctx) throw new Error("canvas 2D context unavailable");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  return ctx;
}
