// X / Y axis pickers, transform selectors and the transform-parameter popover.
import { api } from "../ipc";
import { applyWorkspace, channelLabel, compAvailable, isScatterName, on, sample, setAxes, state } from "../state";
import type { GateDim, Transform, TransformKind } from "../types";
import { clear, h } from "./dom";
import { popover } from "./modal";
import { run, status } from "./status";

const KINDS: TransformKind[] = ["linear", "log", "asinh", "logicle"];
const PARAMS: Record<TransformKind, [string, string][]> = {
  linear: [["t", "T — top of scale"], ["a", "A — offset below zero"]],
  log: [["t", "T — top of scale"], ["m", "M — decades"]],
  asinh: [["t", "T — top of scale"], ["m", "M — decades"], ["a", "A — extra negative decades"]],
  logicle: [["t", "T — top of scale"], ["w", "W — linear width (decades)"], ["m", "M — decades"], ["a", "A — extra negative decades"]],
};

interface AxisCtl {
  root: HTMLElement;
  channel: HTMLSelectElement;
  kind: HTMLSelectElement;
  params: HTMLButtonElement;
  comp: HTMLInputElement;
  compLabel: HTMLElement;
}

let xc: AxisCtl;
let yc: AxisCtl;

export function transformOf(ch: string): Transform | null {
  return state.ws?.channel_settings[ch]?.transform ?? null;
}

function makeAxis(axis: "x" | "y"): AxisCtl {
  const channel = h("select", { class: "input select", title: `${axis.toUpperCase()} channel` });
  const kind = h("select", { class: "input select small", title: "Axis transform" }, ...KINDS.map((k) => h("option", { value: k, text: k })));
  const params: HTMLButtonElement = h("button", { class: "icon-btn", text: "⚙", title: "Transform parameters…" });
  const comp = h("input", { type: "checkbox" });
  const compLabel = h("label", { class: "check-row compact", title: "Use compensated values" }, comp, " Comp");
  const root = h("div", { class: "axis-ctl" }, h("span", { class: "axis-name", text: axis.toUpperCase() }), channel, kind, params, compLabel);

  const cur = (): GateDim | null => (axis === "x" ? state.x : state.y);
  channel.addEventListener("change", () => {
    if (channel.value === "__hist") return setAxes(null, null, true);
    const d: GateDim = { channel: channel.value, compensated: compAvailable() && !isScatterName(channel.value) };
    if (axis === "x") setAxes(d, null);
    else setAxes(null, d, false);
  });
  comp.addEventListener("change", () => {
    const d = cur();
    if (!d) return;
    const nd = { ...d, compensated: comp.checked };
    if (axis === "x") setAxes(nd, null);
    else setAxes(null, nd);
  });
  kind.addEventListener("change", () => run(async () => {
    const d = cur();
    const s = sample();
    if (!d || !s) return;
    const t = await api.channelDefaultTransform(d.channel, kind.value as TransformKind, s.id);
    await applyWorkspace(await api.channelSetTransform(d.channel, t));
    status.info(`${d.channel}: ${kind.value} transform`);
  }));
  params.addEventListener("click", () => {
    const d = cur();
    if (d) openParams(params, d.channel);
  });
  return { root, channel, kind, params, comp, compLabel };
}

function openParams(anchor: HTMLElement, ch: string): void {
  const t = transformOf(ch);
  if (!t) return status.error(`No transform set for ${ch}`);
  const inputs = PARAMS[t.kind].map(([k, label]) => {
    const inp = h("input", { type: "number", class: "input num", step: "any", value: String((t as unknown as Record<string, number>)[k]) });
    return { k, inp, row: h("label", { class: "field-inline" }, h("span", { text: label }), inp) };
  });
  let close = () => {};
  const apply = () => run(async () => {
    const nt = { ...t } as unknown as Record<string, number | string>;
    for (const { k, inp } of inputs) {
      const v = Number(inp.value);
      if (!isFinite(v)) throw new Error(`Invalid value for ${k.toUpperCase()}`);
      nt[k] = v;
    }
    close();
    await applyWorkspace(await api.channelSetTransform(ch, nt as unknown as Transform));
  });
  const reset = () => run(async () => {
    const s = sample();
    if (!s) return;
    close();
    await applyWorkspace(await api.channelSetTransform(ch, await api.channelDefaultTransform(ch, t.kind, s.id)));
  });
  const body = h("form", { class: "param-pop", onsubmit: (e: Event) => (e.preventDefault(), apply()) },
    h("div", { class: "pop-title", text: `${ch} — ${t.kind}` }), ...inputs.map((i) => i.row),
    h("div", { class: "pop-buttons" },
      h("button", { type: "button", class: "btn", text: "Defaults", onclick: reset }),
      h("button", { type: "submit", class: "btn primary", text: "Apply" })));
  close = popover(anchor, body);
  inputs[0]?.inp.focus();
}

export function initAxes(root: HTMLElement): void {
  xc = makeAxis("x");
  yc = makeAxis("y");
  root.append(yc.root, xc.root);
  on("workspace", refresh);
  on("selection", refresh);
  on("axes", refresh);
  refresh();
}

function fill(ctl: AxisCtl, d: GateDim | null, withHist: boolean, hist: boolean): void {
  const s = sample();
  clear(ctl.channel);
  if (withHist) ctl.channel.append(h("option", { value: "__hist", text: "Histogram" }));
  for (const c of s?.channels ?? []) ctl.channel.append(h("option", { value: c.name, text: channelLabel(c.name, s) }));
  ctl.channel.value = hist ? "__hist" : d?.channel ?? "";
  ctl.channel.disabled = !s;
  const t = d && !hist ? transformOf(d.channel) : null;
  ctl.kind.value = t?.kind ?? "linear";
  ctl.kind.disabled = !t && !(d && !hist);
  ctl.params.disabled = !t;
  const ca = compAvailable() && !hist && !!d;
  ctl.comp.disabled = !ca;
  ctl.comp.checked = ca && !!d?.compensated;
  ctl.compLabel.classList.toggle("disabled", !ca);
  ctl.compLabel.title = compAvailable() ? "Use compensated values" : "No compensation matrix is active";
}

function refresh(): void {
  fill(xc, state.x, false, false);
  fill(yc, state.y, true, state.hist);
}
