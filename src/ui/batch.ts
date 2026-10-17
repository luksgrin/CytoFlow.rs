// Batch view: the current X / Y / population plot for every sample, as small multiples.
import { api } from "../ipc";
import { readTheme, renderPlot, type PlotData } from "../plot/render";
import { makeFrame, setupCanvas } from "../plot/scale";
import { emit, gatePath, group, isLoaded, sampleGate, selectSample, state } from "../state";
import type { PopulationStats } from "../types";
import { clear, fmtInt, fmtPct, h, pct } from "./dom";
import { drawGates } from "./gatedraw";
import { run } from "./status";

let root: HTMLElement;
let seq = 0;
let timer = 0;
const CELL = 190;

export function initBatch(el: HTMLElement): void {
  root = el;
}

export function refreshBatch(): void {
  window.clearTimeout(timer);
  timer = window.setTimeout(build, 30);
}

async function build(): Promise<void> {
  const my = ++seq;
  clear(root);
  const ws = state.ws;
  const x = state.x, y = state.y;
  if (!ws?.samples.length || !x) {
    root.append(h("div", { class: "empty", text: "No samples to show." }));
    return;
  }
  const yLabel = state.hist ? "Histogram" : y?.channel ?? "";
  // When a group is selected, show only its members.
  const grp = group();
  const shown = grp ? ws.samples.filter((s) => grp.sample_ids.includes(s.id)) : ws.samples;
  root.append(h("div", { class: "batch-head", text: `${gatePath(state.gateId)} — ${x.channel} × ${yLabel} (${shown.length} samples)` }));
  const grid = h("div", { class: "batch-grid" });
  root.append(grid);
  const theme = readTheme();
  const cells = shown.map((s) => {
    const canvas = h("canvas");
    const cap = h("div", { class: "batch-cap" }, h("span", { class: "name", text: s.name, title: s.name }), h("span", { class: "num", text: "" }));
    const cell = h("div", {
      class: `batch-cell${s.id === state.sampleId ? " selected" : ""}`, title: `Open ${s.name} in the graph window`,
      onclick: () => run(async () => {
        await selectSample(s.id);
        state.view = "graph";
        emit("view");
      }),
    }, canvas, cap);
    grid.append(cell);
    const ctx = setupCanvas(canvas, CELL, CELL);
    const frame = makeFrame(CELL, CELL, window.devicePixelRatio || 1, true);
    renderPlot(ctx, frame, null, { kind: state.plotKind }, theme, isLoaded(s.id) ? "Loading…" : "Not loaded");
    return { s, ctx, frame, cap };
  });
  // Fetch sequentially so the backend is not flooded; abort if inputs change.
  for (const c of cells) {
    if (my !== seq) return;
    if (!isLoaded(c.s.id)) continue;
    if (state.gateId && !sampleGate(state.gateId, c.s.id)) {
      renderPlot(c.ctx, c.frame, null, { kind: state.plotKind }, theme, "No such gate in this sample");
      continue;
    }
    try {
      const stats = await api.populationStats(c.s.id);
      let data: PlotData;
      if (state.hist) data = { kind: "hist", hist: await api.plotHist(c.s.id, state.gateId, x, 128) };
      else data = { kind: "2d", plot: await api.plot2d(c.s.id, state.gateId, x, y!, 128, 6000) };
      if (my !== seq) return;
      renderPlot(c.ctx, c.frame, data, { kind: state.plotKind }, theme);
      const lookup = (id: string | null): PopulationStats | null => stats.find((r) => r.gate_id === id) ?? null;
      drawGates(c.ctx, c.frame, { x, y: state.hist ? null : y }, state.gateId, lookup, { compact: true });
      const st = lookup(state.gateId);
      const num = c.cap.querySelector(".num");
      if (num && st) num.textContent = state.gateId ? `${fmtPct(pct(st.count, st.parent_count))}% · ${fmtInt(st.count)}` : fmtInt(st.count);
    } catch (e) {
      if (my !== seq) return;
      renderPlot(c.ctx, c.frame, null, { kind: state.plotKind }, theme, "Error");
      c.cap.title = e instanceof Error ? e.message : String(e);
    }
  }
}
