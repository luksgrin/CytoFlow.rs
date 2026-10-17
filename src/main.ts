// Bootstrap: build the UI and load the backend's current workspace.
import "./styles.css";
import { api, isTauri } from "./ipc";
import { applyWorkspace } from "./state";
import { initGraph } from "./ui/graph";
import { initShortcuts } from "./ui/shortcuts";
import { initStats } from "./ui/stats";
import { initStatus, run, status } from "./ui/status";
import { initTheme } from "./ui/theme";
import { initToolbar } from "./ui/toolbar";
import { initWorkspacePanel } from "./ui/workspace";

function el(id: string): HTMLElement {
  const e = document.getElementById(id);
  if (!e) throw new Error(`missing #${id}`);
  return e;
}

async function boot(): Promise<void> {
  initTheme();
  initStatus(el("statusbar"));
  initToolbar(el("toolbar"));
  initWorkspacePanel(el("left"));
  initGraph(el("center"));
  initStats(el("right"));
  initShortcuts();
  window.addEventListener("error", (e) => status.error(`UI error: ${e.message}`));
  window.addEventListener("unhandledrejection", (e) => status.error(`UI error: ${String(e.reason?.message ?? e.reason)}`));
  await run(async () => {
    await applyWorkspace(await api.workspaceGet());
    status.info(isTauri ? "Ready" : "Running in the browser with synthetic mock data");
  });
}

void boot();
