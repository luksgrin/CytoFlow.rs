// Display-space <-> pixel mapping. Both axes cover display space [-0.02, 1.02].
export const D0 = -0.02;
export const D1 = 1.02;
export const DSPAN = D1 - D0;

export interface Frame {
  cssW: number;
  cssH: number;
  dpr: number;
  left: number; // plot area, CSS px
  top: number;
  w: number;
  h: number;
  compact: boolean;
}

export function makeFrame(cssW: number, cssH: number, dpr: number, compact = false): Frame {
  const m = compact ? { l: 6, r: 6, t: 6, b: 6 } : { l: 62, r: 16, t: 14, b: 48 };
  return {
    cssW, cssH, dpr, compact,
    left: m.l, top: m.t,
    w: Math.max(10, cssW - m.l - m.r),
    h: Math.max(10, cssH - m.t - m.b),
  };
}

export const toPx = (f: Frame, v: number) => f.left + ((v - D0) / DSPAN) * f.w;
export const toPy = (f: Frame, v: number) => f.top + f.h - ((v - D0) / DSPAN) * f.h;
export const fromPx = (f: Frame, px: number) => D0 + ((px - f.left) / f.w) * DSPAN;
export const fromPy = (f: Frame, py: number) => D0 + ((f.top + f.h - py) / f.h) * DSPAN;
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
