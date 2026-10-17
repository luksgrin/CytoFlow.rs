// Keywords dialog and Boolean gate dialog.
import { api } from "../ipc";
import { applyWorkspace, gatePath, nextGateName, selectPopulation, state } from "../state";
import type { GateShape } from "../types";
import { clear, h } from "./dom";
import { modal } from "./modal";
import { run, status } from "./status";

export const openKeywords = (sampleId: string) => run(async () => {
  const s = state.ws?.samples.find((x) => x.id === sampleId);
  const kw = await api.sampleKeywords(sampleId);
  const entries = Object.entries(kw).sort(([a], [b]) => a.localeCompare(b, undefined, { numeric: true }));
  const filter = h("input", { type: "search", class: "input", placeholder: "Filter keywords…" });
  const tbody = h("tbody");
  const fill = () => {
    clear(tbody);
    const q = filter.value.toLowerCase();
    for (const [k, v] of entries) {
      if (q && !k.toLowerCase().includes(q) && !v.toLowerCase().includes(q)) continue;
      tbody.append(h("tr", {}, h("td", { class: "kw-key", text: k }), h("td", { class: "kw-val", text: v, title: v })));
    }
  };
  filter.addEventListener("input", fill);
  fill();
  const body = h("div", { class: "kw" }, filter,
    h("div", { class: "table-scroll tall" }, h("table", { class: "grid" },
      h("thead", {}, h("tr", {}, h("th", { text: "Keyword" }), h("th", { text: "Value" }))), tbody)),
    h("p", { class: "muted", text: `${entries.length} keywords` }));
  await modal(`Keywords — ${s?.name ?? sampleId}`, body, [{ label: "Close", value: "ok", primary: true }], { wide: true });
});

export const openBooleanDialog = () => run(async () => {
  const gates = state.ws?.gates ?? [];
  if (!gates.length) return status.error("Create some gates first");
  const op = h("select", { class: "input" },
    h("option", { value: "and", text: "AND — in all selected" }),
    h("option", { value: "or", text: "OR — in any selected" }),
    h("option", { value: "not", text: "NOT — in none of the selected" }));
  const name = h("input", { type: "text", class: "input", value: nextGateName("Boolean") });
  const boxes = gates.map((g) => {
    const cb = h("input", { type: "checkbox", value: g.id, checked: g.id === state.gateId });
    return { cb, row: h("label", { class: "check-row" }, cb, ` ${gatePath(g.id)}`) };
  });
  const body = h("div", { class: "form" },
    h("label", { class: "field" }, h("span", { text: "Name" }), name),
    h("label", { class: "field" }, h("span", { text: "Operation" }), op),
    h("div", { class: "field" }, h("span", { text: "Populations" }), h("div", { class: "check-list" }, ...boxes.map((b) => b.row))),
    h("p", { class: "muted", text: `The new population is created under “${gatePath(state.gateId)}”.` }));
  const r = await modal("Boolean gate", body, [{ label: "Cancel", value: "c" }, { label: "Create", value: "ok", primary: true }]);
  if (r !== "ok") return;
  const refs = boxes.filter((b) => b.cb.checked).map((b) => b.cb.value);
  const o = op.value as "and" | "or" | "not";
  if (!refs.length || (o !== "not" && refs.length < 2)) return status.error(o === "not" ? "Select at least one population" : "Select at least two populations");
  const shape: GateShape = { type: "boolean", op: o, refs };
  const res = await api.gateAdd(name.value.trim() || nextGateName("Boolean"), state.gateId, shape, false);
  await applyWorkspace(res.workspace);
  selectPopulation(res.id);
  status.ok(`Created ${name.value}`);
});
