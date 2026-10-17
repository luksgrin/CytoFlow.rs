// Compensation dialog: source radio + editable spillover grid.
import { api } from "../ipc";
import { applyWorkspace, isScatterName, sample, state } from "../state";
import type { CompMatrix, CompSource } from "../types";
import { clear, h } from "./dom";
import { modal } from "./modal";
import { run, status } from "./status";

/** Parse a $SPILLOVER / SPILL keyword: "n,ch1..chn,v11..vnn". */
export function parseSpill(text: string | undefined): CompMatrix | null {
  if (!text) return null;
  const p = text.split(",").map((s) => s.trim());
  const n = parseInt(p[0], 10);
  if (!(n > 0) || p.length < 1 + n + n * n) return null;
  const channels = p.slice(1, 1 + n);
  const vals = p.slice(1 + n, 1 + n + n * n).map(Number);
  if (vals.some((v) => !isFinite(v))) return null;
  return { channels, spillover: channels.map((_, i) => vals.slice(i * n, i * n + n)) };
}

function identity(channels: string[]): CompMatrix {
  return { channels, spillover: channels.map((_, i) => channels.map((__, j) => (i === j ? 1 : 0))) };
}

export const openCompensation = () => run(async () => {
  const s = sample();
  if (!s || !state.ws) return status.error("Select a sample first");
  const effective = state.ws.loaded.includes(s.id) ? await api.compensationMatrix(s.id) : null;
  let fcs: CompMatrix | null = state.ws.compensation.source === "fcs" ? effective : null;
  if (!fcs) {
    const kw = state.ws.loaded.includes(s.id) ? await api.sampleKeywords(s.id).catch(() => ({} as Record<string, string>)) : {};
    fcs = parseSpill(kw["$SPILLOVER"] ?? kw["SPILL"] ?? kw["$SPILL"] ?? kw["SPILLOVER"]);
  }
  const cur = state.ws.compensation;
  let matrix: CompMatrix = structuredClone(cur.source === "custom" ? cur.matrix : effective ?? fcs ??
    identity(s.channels.map((c) => c.name).filter((c) => !isScatterName(c))));
  let source: CompSource["source"] = cur.source;

  const radios = (["fcs", "none", "custom"] as const).map((v) => {
    const r = h("input", { type: "radio", name: "comp-src", value: v, checked: v === source, disabled: v === "fcs" && !fcs });
    r.addEventListener("change", () => {
      source = v;
      if (v === "fcs" && fcs) matrix = structuredClone(fcs);
      renderGrid();
    });
    const labels = { fcs: "Use FCS matrix ($SPILLOVER)", none: "No compensation", custom: "Custom matrix" };
    return { v, r, row: h("label", { class: "check-row" }, r, ` ${labels[v]}${v === "fcs" && !fcs ? " — none in file" : ""}` ) };
  });
  const gridWrap = h("div", { class: "table-scroll comp-grid" });
  const note = h("p", { class: "muted" });

  function setCustom(): void {
    if (source !== "custom") {
      source = "custom";
      radios.forEach((x) => (x.r.checked = x.v === "custom"));
      note.textContent = "Edited — will be saved as a custom matrix.";
    }
  }

  function renderGrid(): void {
    clear(gridWrap);
    const disabled = source === "none";
    const table = h("table", { class: "grid matrix" });
    table.append(h("thead", {}, h("tr", {}, h("th", { class: "corner", text: "spill from \\ into" }),
      ...matrix.channels.map((c) => h("th", { text: c, title: c })))));
    const tb = h("tbody");
    matrix.spillover.forEach((row, i) => {
      tb.append(h("tr", {}, h("th", { text: matrix.channels[i] }), ...row.map((v, j) => {
        const inp = h("input", { type: "number", class: `cell${i === j ? " diag" : ""}${v !== 0 && i !== j ? " nz" : ""}`, step: "0.001", value: String(+v.toFixed(5)), disabled });
        inp.addEventListener("input", () => {
          const n = Number(inp.value);
          if (!isFinite(n)) return;
          matrix.spillover[i][j] = n;
          inp.classList.toggle("nz", n !== 0 && i !== j);
          setCustom();
        });
        return h("td", {}, inp);
      })));
    });
    table.append(tb);
    gridWrap.append(table);
    note.textContent = source === "none" ? "Compensated axes will be disabled." : source === "fcs" ? "Each sample uses the matrix stored in its own FCS file (shown: selected sample)." : "One custom matrix is applied to all samples.";
  }
  renderGrid();

  const body = h("div", { class: "form" }, h("div", { class: "radio-row" }, ...radios.map((x) => x.row)), gridWrap, note);
  if ((await modal("Compensation", body, [{ label: "Cancel", value: "c" }, { label: "Apply", value: "ok", primary: true }], { wide: true })) !== "ok") return;
  const comp: CompSource = source === "custom" ? { source, matrix } : { source };
  await applyWorkspace(await api.compensationSet(comp));
  status.ok(`Compensation: ${source}`);
});
