// Zoom and pan of the graph window: mouse wheel zooms around the cursor,
// right-drag, middle-drag or space+drag pans. The visible window is kept in
// display space and clamped to the full axis range; the graph re-fetches the
// plot for the new window (re-binned at full resolution by the backend).
import { D0, D1, DSPAN, type Frame, fromPx, fromPy, type Range } from "../plot/scale";

export interface ZoomHost {
  frame(): Frame;
  /** Histograms zoom along x only. */
  isHist(): boolean;
  /** Whether there is a plot to zoom. */
  enabled(): boolean;
  /** Apply a new visible window. */
  set(xr: Range, yr: Range): void;
  /** Cursor of the current tool, restored after a pan. */
  restCursor(): string;
}

/** Smallest visible span (display units): ~1/250 of the axis. */
export const MIN_SPAN = 0.004;

/** Keep a window inside the full axis, at least MIN_SPAN wide. */
export function clampRange(r: Range): Range {
  let span = Math.min(DSPAN, Math.max(MIN_SPAN, r[1] - r[0]));
  if (!Number.isFinite(span)) span = DSPAN;
  let lo = Number.isFinite(r[0]) ? r[0] : D0;
  if (r[1] - r[0] < MIN_SPAN) lo = (r[0] + r[1]) / 2 - span / 2;
  lo = Math.min(D1 - span, Math.max(D0, lo));
  return [lo, lo + span];
}

/** Zoom `r` by `factor` (< 1 = in) keeping display value `at` fixed. */
export function zoomAround(r: Range, at: number, factor: number): Range {
  const span = Math.min(DSPAN, Math.max(MIN_SPAN, (r[1] - r[0]) * factor));
  const t = (at - r[0]) / (r[1] - r[0]);
  return clampRange([at - t * span, at - t * span + span]);
}

interface Pan { id: number; x: number; y: number; f: Frame; moved: boolean; button: number }

let pan: Pan | null = null;
let spaceDown = false;
let lastPanEnd = 0;
let deferredMenu: { x: number; y: number } | null = null;
let replaying = false;

const typingTarget = (t: EventTarget | null) => {
  const el = t as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable);
};

export const panning = () => !!pan;

/**
 * Attach zoom / pan handling to the plot's top canvas. Must be called before
 * any other pointer listeners are added to `c` (pans swallow their events).
 */
export function initZoom(c: HTMLCanvasElement, host: ZoomHost): void {
  const local = (e: MouseEvent) => {
    const r = c.getBoundingClientRect();
    return { px: e.clientX - r.left, py: e.clientY - r.top };
  };
  const inPlot = (f: Frame, px: number, py: number) => px >= f.left && px <= f.left + f.w && py >= f.top && py <= f.top + f.h;

  c.addEventListener("wheel", (e) => {
    if (!host.enabled()) return;
    const f = host.frame();
    const { px, py } = local(e);
    if (!inPlot(f, px, py)) return;
    e.preventDefault();
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1;
    // Trackpad pinch arrives as ctrl+wheel with small deltas: zoom faster.
    const k = e.ctrlKey ? 0.01 : 0.0015;
    const factor = Math.exp(Math.max(-1, Math.min(1, e.deltaY * unit * k)));
    const xr = zoomAround(f.xr, fromPx(f, px), factor);
    const yr = host.isHist() ? f.yr : zoomAround(f.yr, fromPy(f, py), factor);
    host.set(xr, yr);
  }, { passive: false });

  c.addEventListener("pointerdown", (e) => {
    const wantPan = e.button === 2 || e.button === 1 || (e.button === 0 && spaceDown);
    if (!wantPan || !host.enabled()) return;
    const f = host.frame();
    const { px, py } = local(e);
    if (!inPlot(f, px, py)) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    pan = { id: e.pointerId, x: px, y: py, f, moved: false, button: e.button };
    c.setPointerCapture(e.pointerId);
    c.style.cursor = "grabbing";
  });

  c.addEventListener("pointermove", (e) => {
    if (!pan || e.pointerId !== pan.id) return;
    e.stopImmediatePropagation();
    const { px, py } = local(e);
    const dx = px - pan.x, dy = py - pan.y;
    if (!pan.moved && Math.hypot(dx, dy) < 3) return;
    pan.moved = true;
    const f = pan.f;
    const sx = ((f.xr[1] - f.xr[0]) * dx) / f.w, sy = ((f.yr[1] - f.yr[0]) * dy) / f.h;
    const xr = clampRange([f.xr[0] - sx, f.xr[1] - sx]);
    const yr = host.isHist() ? f.yr : clampRange([f.yr[0] + sy, f.yr[1] + sy]);
    host.set(xr, yr);
  });

  const end = (e: PointerEvent) => {
    if (!pan || e.pointerId !== pan.id) return;
    e.stopImmediatePropagation();
    if (c.hasPointerCapture(e.pointerId)) c.releasePointerCapture(e.pointerId);
    const p = pan;
    pan = null;
    c.style.cursor = spaceDown ? "grab" : host.restCursor();
    if (p.moved) lastPanEnd = performance.now();
    // A right click without movement still opens the gate context menu
    // (on macOS `contextmenu` fires on press, before we know it was a click).
    if (p.button === 2 && !p.moved && deferredMenu) {
      const at = deferredMenu;
      deferredMenu = null;
      replaying = true;
      try {
        c.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: at.x, clientY: at.y }));
      } finally {
        replaying = false;
      }
    }
    deferredMenu = null;
  };
  c.addEventListener("pointerup", end);
  c.addEventListener("pointercancel", end);

  c.addEventListener("contextmenu", (e) => {
    if (replaying) return;
    if (pan && pan.button === 2) {
      // Pressed, not yet released: decide on release.
      e.preventDefault();
      e.stopImmediatePropagation();
      deferredMenu = { x: e.clientX, y: e.clientY };
      return;
    }
    if (performance.now() - lastPanEnd < 400) {
      // Fired on release after a drag (Windows / Linux): it was a pan.
      e.preventDefault();
      e.stopImmediatePropagation();
    }
  });

  document.addEventListener("keydown", (e) => {
    if (e.key !== " " || typingTarget(e.target) || (e.target as HTMLElement | null)?.tagName === "BUTTON" || e.metaKey || e.ctrlKey) return;
    e.preventDefault();
    if (!spaceDown) {
      spaceDown = true;
      if (!pan) c.style.cursor = "grab";
    }
  });
  document.addEventListener("keyup", (e) => {
    if (e.key !== " ") return;
    spaceDown = false;
    if (!pan) c.style.cursor = host.restCursor();
  });
  window.addEventListener("blur", () => (spaceDown = false));
}
