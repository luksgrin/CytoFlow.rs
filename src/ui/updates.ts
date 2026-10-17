// In-app updates: "Check for updates…" toolbar button, a daily automatic check,
// and the install dialog. The version policy (same major only, no pre-releases,
// no self-update of .deb installs) is enforced by the backend (src-tauri/src/updates.rs);
// this file only presents what `update_check` reports.
import { call, isTauri } from "../ipc";
import { h } from "./dom";
import { modal } from "./modal";
import { status } from "./status";

export interface UpdateInfo {
  available: boolean;
  version: string | null;
  currentVersion: string;
  notes: string | null;
  date: string | null;
  skipped: boolean;
  reason?: "major" | "prerelease" | "deb" | null;
  url?: string | null;
}
interface UpdateProgress { downloaded: number; total: number | null; finished: boolean }

const RELEASES = "https://github.com/luksgrin/CytoFlow.rs/releases";
const KEY_AUTO = "cytoflow.updates.auto";
const KEY_LAST = "cytoflow.updates.lastCheck";
const KEY_SKIP = "cytoflow.updates.skip";
const DAY_MS = 24 * 60 * 60 * 1000;
const STARTUP_DELAY_MS = 10_000;

function load(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function store(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    /* storage unavailable: settings just do not persist */
  }
}

export const autoCheckEnabled = () => load(KEY_AUTO) !== "0";
export const setAutoCheck = (on: boolean) => store(KEY_AUTO, on ? "1" : "0");

const checkUpdate = (skipVersion?: string | null) =>
  call<UpdateInfo>("update_check", skipVersion ? { includePrerelease: false, skipVersion } : { includePrerelease: false });

async function openExternal(url: string): Promise<void> {
  if (isTauri) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else window.open(url, "_blank", "noopener");
}

/** Toolbar button (placed next to the theme toggle). */
export function updatesButton(): HTMLButtonElement {
  return h("button", { class: "tb-btn", text: "⟳ Updates", title: "Check for updates…", onclick: () => void checkForUpdates(true) });
}

/** Schedule the automatic check: 10 s after startup, at most once per 24 h, never in mock mode. */
export function initUpdates(): void {
  if (!isTauri || !autoCheckEnabled()) return;
  const last = Number(load(KEY_LAST) ?? 0);
  if (Date.now() - last < DAY_MS) return;
  window.setTimeout(() => void checkForUpdates(false), STARTUP_DELAY_MS);
}

/** `manual`: from the button (always shows a result, ignores "Skip this version"). */
export async function checkForUpdates(manual: boolean): Promise<void> {
  let info: UpdateInfo;
  try {
    info = await checkUpdate(manual ? null : load(KEY_SKIP));
    store(KEY_LAST, String(Date.now()));
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    if (manual) status.error(`Update check failed: ${msg}`);
    else console.warn(`automatic update check failed: ${msg}`);
    return;
  }
  if (!info.version || (info.skipped && !manual)) {
    status.ok(`CytoFlow ${info.currentVersion} is up to date`);
    if (manual) await showDialog(info);
    return;
  }
  await showDialog(info);
}

function notesBlock(info: UpdateInfo): HTMLElement | null {
  if (!info.notes?.trim()) return null;
  return h("div", { class: "update-notes", text: info.notes.trim() });
}

function settingsRow(): HTMLElement {
  const box = h("input", { type: "checkbox", checked: autoCheckEnabled(), onchange: () => setAutoCheck(box.checked) });
  return h("label", { class: "update-auto" }, box, h("span", { text: "Check for updates automatically" }));
}

async function showDialog(info: UpdateInfo): Promise<void> {
  const cur = info.currentVersion;
  const when = info.date ? new Date(info.date).toLocaleDateString() : null;
  const releaseUrl = info.url ?? (info.version ? `${RELEASES}/tag/v${info.version}` : RELEASES);
  let title: string;
  let lead: (Node | string)[];
  let buttons: { label: string; value: string; primary?: boolean }[];

  if (!info.version) {
    title = "CytoFlow is up to date";
    lead = [h("p", { text: `You have the latest version (${cur}).` })];
    buttons = [{ label: "Close", value: "close", primary: true }];
  } else if (info.available) {
    title = `CytoFlow ${info.version} is available`;
    lead = [h("p", { text: `You have ${cur}.${when ? ` Released ${when}.` : ""}` })];
    buttons = [
      { label: "Skip this version", value: "skip" },
      { label: "Later", value: "later" },
      { label: "Install and restart", value: "install", primary: true },
    ];
  } else {
    title = `CytoFlow ${info.version} is available`;
    const why = info.reason === "major"
      ? `It is a new major version (you have ${cur}), which may change how workspaces or results behave, so it is not installed automatically. Download it from the releases page when you are ready.`
      : info.reason === "deb"
        ? `You installed CytoFlow ${cur} from the Debian package, which the app does not update itself. Download the new .deb from the releases page and install it with apt.`
        : `Version ${info.version} cannot be installed from inside the app. Download it from the releases page.`;
    lead = [h("p", { text: why })];
    buttons = [
      { label: "Skip this version", value: "skip" },
      { label: "Later", value: "later" },
      { label: "Open releases page", value: "open", primary: true },
    ];
  }

  const body = h("div", { class: "update-dialog" },
    ...lead,
    info.version ? notesBlock(info) : null,
    info.version ? h("p", {}, h("a", { href: "#", text: "Release notes on GitHub", onclick: (e: Event) => (e.preventDefault(), void openExternal(releaseUrl)) })) : null,
    settingsRow(),
  );
  const r = await modal(title, body, buttons, { wide: Boolean(info.notes) });
  if (r === "skip" && info.version) {
    store(KEY_SKIP, info.version);
    status.info(`Skipping CytoFlow ${info.version}; you will be told about the next version`);
  } else if (r === "open") {
    await openExternal(releaseUrl);
  } else if (r === "install" && info.version) {
    await install(info.version);
  }
}

async function install(version: string): Promise<void> {
  const bar = h("div", { class: "update-bar" });
  const label = h("div", { class: "update-progress-label muted", text: "Starting download…" });
  const box = h("div", { class: "modal", role: "dialog" },
    h("div", { class: "modal-title", text: `Updating to CytoFlow ${version}` }),
    h("div", { class: "modal-body" }, h("div", { class: "update-track" }, bar), label),
    h("div", { class: "modal-buttons" }));
  const back = h("div", { class: "modal-backdrop" }, box);
  document.body.append(back);

  let unlisten: (() => void) | null = null;
  if (isTauri) {
    const { listen } = await import("@tauri-apps/api/event");
    unlisten = await listen<UpdateProgress>("update-progress", ({ payload: p }) => {
      if (p.finished) {
        bar.style.width = "100%";
        label.textContent = "Installing… CytoFlow restarts when done.";
        return;
      }
      const mb = (n: number) => (n / 1048576).toFixed(1);
      if (p.total) {
        bar.style.width = `${Math.min(100, (100 * p.downloaded) / p.total).toFixed(1)}%`;
        label.textContent = `Downloading… ${mb(p.downloaded)} of ${mb(p.total)} MB`;
      } else {
        bar.classList.add("indeterminate");
        label.textContent = `Downloading… ${mb(p.downloaded)} MB`;
      }
    });
  }
  try {
    // Resolves only on failure: on success the backend relaunches the app.
    await call<null>("update_install", { version, includePrerelease: false });
  } catch (e) {
    status.error(`Update failed: ${e instanceof Error ? e.message : String(e)}`);
  } finally {
    unlisten?.();
    back.remove();
  }
}
