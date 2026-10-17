// Image / SVG export helpers: native save via the backend (export_image,
// write_text_file) inside Tauri, a browser download otherwise; clipboard copy.
import { call, isTauri } from "../ipc";
import { pickSave } from "../ui/files";

function download(href: string, name: string): void {
  const a = document.createElement("a");
  a.href = href;
  a.download = name;
  document.body.append(a);
  a.click();
  a.remove();
}

const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;

/** Save a PNG data URL. Returns the path (or file name in the browser), null if cancelled. */
export async function savePng(dataUrl: string, defaultName: string): Promise<string | null> {
  if (!isTauri) {
    download(dataUrl, baseName(defaultName));
    return baseName(defaultName);
  }
  let path = await pickSave("Export image", [{ name: "PNG image", extensions: ["png"] }], defaultName);
  if (!path) return null;
  if (!/\.png$/i.test(path)) path += ".png";
  await call<null>("export_image", { path, base64Data: dataUrl });
  return path;
}

/** Save an SVG document. Returns the path (or file name in the browser), null if cancelled. */
export async function saveSvg(svg: string, defaultName: string): Promise<string | null> {
  if (!isTauri) {
    const url = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
    download(url, baseName(defaultName));
    setTimeout(() => URL.revokeObjectURL(url), 5000);
    return baseName(defaultName);
  }
  let path = await pickSave("Export SVG", [{ name: "SVG image", extensions: ["svg"] }], defaultName);
  if (!path) return null;
  if (!/\.svg$/i.test(path)) path += ".svg";
  await call<null>("write_text_file", { path, contents: svg });
  return path;
}

/** Stack same-sized canvases (e.g. plot + gate overlay) onto one opaque canvas. */
export function composite(layers: HTMLCanvasElement[], background: string): HTMLCanvasElement {
  const w = layers[0]?.width ?? 1, hgt = layers[0]?.height ?? 1;
  const out = document.createElement("canvas");
  out.width = w;
  out.height = hgt;
  const ctx = out.getContext("2d");
  if (!ctx) throw new Error("canvas 2D context unavailable");
  ctx.fillStyle = background;
  ctx.fillRect(0, 0, w, hgt);
  for (const l of layers) ctx.drawImage(l, 0, 0);
  return out;
}

const toBlob = (c: HTMLCanvasElement) =>
  new Promise<Blob | null>((resolve) => c.toBlob((b) => resolve(b), "image/png"));

/**
 * Copy a canvas to the clipboard as PNG when the webview supports it; otherwise
 * fall back to the export dialog.
 */
export async function copyPng(c: HTMLCanvasElement, fallbackName: string): Promise<"copied" | "saved" | null> {
  const Item = (window as unknown as { ClipboardItem?: typeof ClipboardItem }).ClipboardItem;
  if (Item && navigator.clipboard?.write) {
    try {
      await navigator.clipboard.write([new Item({ "image/png": toBlob(c).then((b) => b ?? new Blob()) })]);
      return "copied";
    } catch {
      /* fall through to the save dialog */
    }
  }
  return (await savePng(c.toDataURL("image/png"), fallbackName)) ? "saved" : null;
}
