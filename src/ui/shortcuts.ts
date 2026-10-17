// Global keyboard shortcuts.
import { gate, selectPopulation, setTool, state, type Tool } from "../state";
import { deleteGate, openFcs, openWorkspace, saveWorkspace } from "./actions";
import { cancelDrawing, closePolygon, deleteSelectedGate } from "./gating";
import { currentView } from "./graph";
import { deleteSelectedPanel } from "./layout";
import { modalOpen } from "./modal";

const TOOL_KEYS: Record<string, Tool> = { v: "select", r: "rect", p: "polygon", e: "ellipse", g: "range", q: "quadrant" };

export function initShortcuts(): void {
  document.addEventListener("keydown", (e) => {
    if (modalOpen()) return;
    const mod = e.metaKey || e.ctrlKey;
    const k = e.key.toLowerCase();
    if (mod && k === "o") return void (e.preventDefault(), e.shiftKey ? openWorkspace() : openFcs());
    if (mod && k === "s") return void (e.preventDefault(), saveWorkspace(e.shiftKey));
    const t = e.target as HTMLElement | null;
    const typing = !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable);
    if (typing) return;
    if (e.key === "Escape") {
      if (cancelDrawing()) e.preventDefault();
      return;
    }
    if (e.key === "Enter" && closePolygon()) return void e.preventDefault();
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      if (currentView() === "layout") return void deleteSelectedPanel();
      if (!deleteSelectedGate() && state.gateId) void deleteGate(state.gateId);
      return;
    }
    if (!mod && !e.altKey && TOOL_KEYS[k]) {
      const tool = TOOL_KEYS[k];
      const twoD = tool !== "range" && tool !== "select";
      if ((state.hist && twoD) || (!state.hist && tool === "range")) return;
      cancelDrawing();
      setTool(tool);
      return;
    }
    if (e.key === "ArrowLeft" && e.altKey && state.gateId) selectPopulation(gate(state.gateId)?.parent ?? null);
  });
}
