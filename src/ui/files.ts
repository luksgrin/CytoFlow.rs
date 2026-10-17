// Native file dialogs (Tauri plugin) with a text-prompt fallback in mock mode.
import { isTauri } from "../ipc";
import { promptText } from "./modal";

export interface Filter { name: string; extensions: string[] }

export async function pickOpen(title: string, filters: Filter[], multiple = false, mockDefault = ""): Promise<string[] | null> {
  if (isTauri) {
    const d = await import("@tauri-apps/plugin-dialog");
    const r = await d.open({ title, filters, multiple, directory: false });
    if (r === null) return null;
    return Array.isArray(r) ? r : [r];
  }
  const v = await promptText(title, multiple ? "Paths (comma separated, mock mode)" : "Path (mock mode)", mockDefault);
  return v ? v.split(",").map((s) => s.trim()).filter(Boolean) : null;
}

export async function pickDir(title: string): Promise<string | null> {
  if (isTauri) {
    const d = await import("@tauri-apps/plugin-dialog");
    const r = await d.open({ title, directory: true, multiple: false });
    return typeof r === "string" ? r : null;
  }
  return promptText(title, "Folder (mock mode)", "/mock/data");
}

export async function pickSave(title: string, filters: Filter[], defaultPath: string): Promise<string | null> {
  if (isTauri) {
    const d = await import("@tauri-apps/plugin-dialog");
    return d.save({ title, filters, defaultPath });
  }
  return promptText(title, "Save to (mock mode)", defaultPath);
}

export const dirname = (p: string) => p.replace(/[\\/][^\\/]*$/, "") || "/";
