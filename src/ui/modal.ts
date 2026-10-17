// Modal dialogs, text prompt, context menus and anchored popovers.
import { h } from "./dom";

let openCount = 0;
export const modalOpen = () => openCount > 0;

export interface ModalButton { label: string; value: string; primary?: boolean; danger?: boolean }

/** Show a modal; resolves with the clicked button's value, or null on Esc / backdrop. */
export function modal(title: string, body: Node, buttons: ModalButton[], opts: { wide?: boolean; onEnter?: string } = {}): Promise<string | null> {
  return new Promise((resolve) => {
    openCount++;
    const close = (v: string | null) => {
      openCount--;
      back.remove();
      document.removeEventListener("keydown", key, true);
      resolve(v);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") (e.preventDefault(), e.stopPropagation(), close(null));
      else if (e.key === "Enter" && opts.onEnter && !(e.target instanceof HTMLTextAreaElement)) {
        e.preventDefault();
        e.stopPropagation();
        close(opts.onEnter);
      }
    };
    const box = h("div", { class: `modal${opts.wide ? " wide" : ""}`, role: "dialog" },
      h("div", { class: "modal-title", text: title }),
      h("div", { class: "modal-body" }, body),
      h("div", { class: "modal-buttons" }, ...buttons.map((b) =>
        h("button", { class: `btn${b.primary ? " primary" : ""}${b.danger ? " danger" : ""}`, text: b.label, onclick: () => close(b.value) }))),
    );
    const back = h("div", { class: "modal-backdrop", onmousedown: (e: Event) => e.target === back && close(null) }, box);
    document.body.append(back);
    document.addEventListener("keydown", key, true);
    const first = box.querySelector<HTMLElement>("input, select, textarea") ?? box.querySelector<HTMLElement>(".btn.primary");
    first?.focus();
    if (first instanceof HTMLInputElement) first.select();
  });
}

export async function promptText(title: string, label: string, value: string): Promise<string | null> {
  const input = h("input", { type: "text", class: "input", value, spellcheck: false });
  const body = h("label", { class: "field" }, h("span", { text: label }), input);
  const r = await modal(title, body, [{ label: "Cancel", value: "cancel" }, { label: "OK", value: "ok", primary: true }], { onEnter: "ok" });
  const v = input.value.trim();
  return r === "ok" && v ? v : null;
}

export async function confirmBox(title: string, text: string, okLabel = "OK", danger = false): Promise<boolean> {
  const r = await modal(title, h("p", { text }), [{ label: "Cancel", value: "cancel" }, { label: okLabel, value: "ok", primary: !danger, danger }], { onEnter: "ok" });
  return r === "ok";
}

export async function alertBox(title: string, body: Node | string): Promise<void> {
  await modal(title, typeof body === "string" ? h("p", { text: body }) : body, [{ label: "OK", value: "ok", primary: true }], { onEnter: "ok" });
}

// ---------------------------------------------------------------- menus / popovers

export interface MenuItem { label: string; action?: () => void; checked?: boolean; disabled?: boolean; danger?: boolean; sep?: boolean }

let floating: HTMLElement | null = null;
function closeFloating(): void {
  floating?.remove();
  floating = null;
}
document.addEventListener("mousedown", (e) => {
  if (floating && !floating.contains(e.target as Node)) closeFloating();
});
document.addEventListener("keydown", (e) => e.key === "Escape" && closeFloating());
window.addEventListener("blur", closeFloating);

function place(el: HTMLElement, x: number, y: number): void {
  document.body.append(el);
  const r = el.getBoundingClientRect();
  el.style.left = `${Math.max(4, Math.min(x, window.innerWidth - r.width - 4))}px`;
  el.style.top = `${Math.max(4, Math.min(y, window.innerHeight - r.height - 4))}px`;
}

export function showMenu(x: number, y: number, items: MenuItem[]): void {
  closeFloating();
  const el = h("div", { class: "menu" }, ...items.map((it) =>
    it.sep ? h("div", { class: "menu-sep" }) :
      h("div", {
        class: `menu-item${it.disabled ? " disabled" : ""}${it.danger ? " danger" : ""}`,
        onclick: () => {
          if (it.disabled) return;
          closeFloating();
          it.action?.();
        },
      }, h("span", { class: "check", text: it.checked ? "✓" : "" }), it.label)));
  floating = el;
  place(el, x, y);
}

/** Show arbitrary content anchored below an element. Returns a close function. */
export function popover(anchor: HTMLElement, content: Node): () => void {
  closeFloating();
  const el = h("div", { class: "popover" }, content);
  floating = el;
  const r = anchor.getBoundingClientRect();
  place(el, r.left, r.bottom + 4);
  return closeFloating;
}
