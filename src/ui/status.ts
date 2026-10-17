// Status bar: messages, errors and a busy indicator.
import { isTauri, onBusy } from "../ipc";
import { h } from "./dom";

let msgEl: HTMLElement;
let busyEl: HTMLElement;
let infoEl: HTMLElement;
let timer = 0;

export function initStatus(root: HTMLElement): void {
  msgEl = h("span", { class: "status-msg", text: "Ready" });
  infoEl = h("span", { class: "status-info" });
  busyEl = h("span", { class: "status-busy", title: "Working…" });
  root.append(busyEl, msgEl, h("span", { class: "spacer" }), infoEl,
    isTauri ? "" : h("span", { class: "badge", text: "Mock data", title: "Running in a browser without the Tauri backend" }));
  onBusy((n) => busyEl.classList.toggle("on", n > 0));
}

function show(text: string, cls: string, sticky: boolean): void {
  if (!msgEl) return;
  msgEl.textContent = text;
  msgEl.className = `status-msg ${cls}`;
  msgEl.title = text;
  window.clearTimeout(timer);
  if (!sticky) timer = window.setTimeout(() => ((msgEl.textContent = "Ready"), (msgEl.className = "status-msg")), 6000);
}

export const status = {
  info: (t: string) => show(t, "", false),
  ok: (t: string) => show(t, "ok", false),
  error: (t: string) => {
    console.error(t);
    show(t, "error", true);
  },
  setInfo: (t: string) => infoEl && (infoEl.textContent = t),
};

/** Run an async action, routing any error to the status bar. */
export async function run<T>(fn: () => Promise<T>): Promise<T | undefined> {
  try {
    return await fn();
  } catch (e) {
    status.error(e instanceof Error ? e.message : String(e));
    return undefined;
  }
}
