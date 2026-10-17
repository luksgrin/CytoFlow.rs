// Minimal DOM helpers.
type Attrs = Record<string, string | number | boolean | EventListener | undefined>;

export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K, attrs: Attrs = {}, ...kids: (Node | string | null | undefined | false)[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === undefined || v === false) continue;
    if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2), v as EventListener);
    else if (k === "class") el.className = String(v);
    else if (k === "text") el.textContent = String(v);
    else if (k in el && typeof v !== "string") (el as unknown as Record<string, unknown>)[k] = v;
    else el.setAttribute(k, v === true ? "" : String(v));
  }
  for (const c of kids) if (c !== null && c !== undefined && c !== false) el.append(c);
  return el;
}

export function clear(el: Element): void {
  while (el.firstChild) el.removeChild(el.firstChild);
}

export const fmtInt = (n: number) => Math.round(n).toLocaleString("en-US");
export const fmtPct = (n: number) => (isFinite(n) ? (n >= 10 ? n.toFixed(1) : n.toFixed(2)) : "–");
export function fmtNum(n: number | null | undefined): string {
  if (n === null || n === undefined || !isFinite(n)) return "–";
  const a = Math.abs(n);
  if (a >= 1e5) return n.toExponential(2);
  if (a >= 100) return n.toFixed(0);
  if (a >= 1) return n.toFixed(2);
  return n.toPrecision(3);
}
export const pct = (count: number, of: number) => (of > 0 ? (100 * count) / of : 0);
