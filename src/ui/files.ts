// Native file dialogs (Tauri plugin) with a text-prompt fallback in mock mode.
import { isTauri } from "../ipc";
import type { ExportFormat } from "../types";
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

/** Population export formats: label, save-dialog filter, extension. */
export const EXPORT_FORMATS: Record<ExportFormat, { label: string; hint: string; filter: Filter[]; ext: string }> = {
  csv: { label: "CSV", hint: "Plain text, one column per channel", filter: [{ name: "CSV", extensions: ["csv"] }], ext: "csv" },
  xlsx: {
    label: "Excel (.xlsx)", hint: "“metadata” sheet (keywords, population, settings) + “data” sheet",
    filter: [{ name: "Excel workbook", extensions: ["xlsx"] }], ext: "xlsx",
  },
  parquet: {
    label: "Parquet", hint: "Columnar, one float column per channel (pandas / polars / R arrow)",
    filter: [{ name: "Apache Parquet", extensions: ["parquet"] }], ext: "parquet",
  },
};

/** Replace (or add) the extension of a file name / path. */
export const withExt = (p: string, ext: string) => `${p.replace(/\.(csv|xlsx|parquet)$/i, "")}.${ext}`;

export const dirname = (p: string) => p.replace(/[\\/][^\\/]*$/, "") || "/";
