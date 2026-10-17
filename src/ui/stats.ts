// Right column: population statistics table and per-channel statistics.
import { api } from "../ipc";
import { channelLabel, gate, gatePath, isLoaded, on, sample, sampleGate, selectPopulation, state } from "../state";
import type { ChannelStats } from "../types";
import { exportStats } from "./actions";
import { openCompensation } from "./compensation";
import { openKeywords } from "./dialogs";
import { clear, fmtInt, fmtNum, fmtPct, h, pct } from "./dom";
import { status } from "./status";
import { gateColor } from "./workspace";

let popBody: HTMLElement;
let chanBox: HTMLElement;
let compInfo: HTMLElement;
const cache = new Map<string, Promise<ChannelStats>>();

export function initStats(root: HTMLElement): void {
  popBody = h("tbody");
  chanBox = h("div", { class: "chan-stats" });
  compInfo = h("span", { class: "muted" });
  root.append(
    h("div", { class: "section-head" }, h("span", { text: "Statistics" }), h("span", { class: "spacer" }),
      h("button", { class: "btn small", text: "Export…", title: "Export the statistics table for all samples", onclick: () => exportStats() })),
    h("div", { class: "table-scroll stats-table" }, h("table", { class: "grid" },
      h("thead", {}, h("tr", {}, h("th", { text: "Population" }), h("th", { class: "r", text: "Count" }), h("th", { class: "r", text: "% Parent" }), h("th", { class: "r", text: "% Total" }))),
      popBody)),
    h("div", { class: "section-head" }, h("span", { text: "Selected population" })),
    chanBox,
    h("div", { class: "section-head" }, h("span", { text: "Sample" })),
    h("div", { class: "sample-tools" },
      h("button", { class: "btn", text: "Compensation…", onclick: () => openCompensation() }),
      h("button", { class: "btn", text: "Keywords…", onclick: () => state.sampleId && openKeywords(state.sampleId) }),
      compInfo),
  );
  on("workspace", () => (cache.clear(), renderChannels(), renderComp()));
  on("stats", renderPops);
  on("selection", () => (renderPops(), renderChannels()));
  on("axes", renderChannels);
  renderPops();
  renderComp();
}

function renderComp(): void {
  const c = state.ws?.compensation.source ?? "none";
  compInfo.textContent = c === "none" ? "Uncompensated" : c === "fcs" ? "Comp: FCS matrix" : "Comp: custom";
}

function renderPops(): void {
  clear(popBody);
  // Population statistics are computed on the plotted sample's own tree.
  const depth = (id: string | null) => {
    let d = 0;
    for (let g = sampleGate(id); g; g = sampleGate(g.parent)) d++;
    return d;
  };
  const path = (id: string | null) => {
    const parts: string[] = [];
    for (let g = sampleGate(id); g; g = sampleGate(g.parent)) parts.unshift(g.name);
    return parts.length ? parts.join(" / ") : "All events";
  };
  for (const st of state.popStats) {
    const g = sampleGate(st.gate_id);
    // Rows of populations that are not in the viewed tree (e.g. sample-only
    // gates while a group is selected) cannot be selected there.
    const selectable = !st.gate_id || !!gate(st.gate_id);
    popBody.append(h("tr", {
      class: `${st.gate_id === state.gateId ? "selected" : ""}${g?.tailored ? " tailored" : ""}${selectable ? "" : " outside"}`,
      onclick: () => selectable && selectPopulation(st.gate_id),
      title: `${path(st.gate_id)}${g?.tailored ? " (tailored)" : ""}`,
    },
      h("td", { style: `padding-left:${6 + depth(st.gate_id) * 10}px` },
        g ? h("span", { class: "swatch-dot", style: `background:${gateColor(g)}` }) : "", ` ${st.name}`),
      h("td", { class: "r", text: fmtInt(st.count) }),
      h("td", { class: "r", text: st.gate_id ? fmtPct(pct(st.count, st.parent_count)) : "" }),
      h("td", { class: "r", text: fmtPct(pct(st.count, st.total)) })));
  }
  if (!state.popStats.length) popBody.append(h("tr", {}, h("td", { colspan: 4, class: "empty", text: sample() ? "No statistics (sample not loaded)" : "No sample" })));
}

let chanSeq = 0;
function renderChannels(): void {
  const my = ++chanSeq;
  clear(chanBox);
  const sid = state.sampleId;
  if (!sid || !isLoaded(sid) || !state.x) return void chanBox.append(h("div", { class: "empty", text: "—" }));
  chanBox.append(h("div", { class: "pop-name", text: gatePath(state.gateId) }));
  const dims = [state.x, ...(state.hist || !state.y ? [] : [state.y])];
  const tbody = h("tbody");
  chanBox.append(h("table", { class: "grid" },
    h("thead", {}, h("tr", {}, h("th", { text: "Channel" }), h("th", { class: "r", text: "Median" }), h("th", { class: "r", text: "Mean" }), h("th", { class: "r", text: "CV %" }))),
    tbody));
  for (const d of dims) {
    const key = `${sid}|${state.gateId}|${d.channel}|${d.compensated}`;
    let p = cache.get(key);
    if (!p) {
      p = api.channelStats(sid, state.gateId, d.channel, d.compensated);
      cache.set(key, p);
      p.catch(() => cache.delete(key));
    }
    const row = h("tr", {}, h("td", { text: `${d.compensated ? "Comp " : ""}${channelLabel(d.channel)}` }),
      h("td", { class: "r", text: "…" }), h("td", { class: "r", text: "…" }), h("td", { class: "r", text: "…" }));
    tbody.append(row);
    p.then((cs) => {
      if (my !== chanSeq) return;
      const tds = row.querySelectorAll("td");
      tds[1].textContent = fmtNum(cs.median);
      tds[2].textContent = fmtNum(cs.mean);
      tds[3].textContent = fmtNum(cs.cv);
      row.title = `SD ${fmtNum(cs.sd)} · geo mean ${fmtNum(cs.geo_mean)} · min ${fmtNum(cs.min)} · max ${fmtNum(cs.max)}`;
    }).catch((e) => my === chanSeq && status.error(e instanceof Error ? e.message : String(e)));
  }
}
