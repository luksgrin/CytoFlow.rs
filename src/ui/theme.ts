// Auto (system) / light / dim / dark theme handling. "dim" is a low-contrast
// slate dark theme; renderers treat it as dark.
import { emit } from "../state";

export type ThemePref = "system" | "light" | "dim" | "dark";
const ORDER: ThemePref[] = ["system", "light", "dim", "dark"];
const KEY = "cytoflow.theme";
const mq = window.matchMedia("(prefers-color-scheme: dark)");

let pref: ThemePref = "system";
try {
  const v = localStorage.getItem(KEY);
  if (ORDER.includes(v as ThemePref)) pref = v as ThemePref;
} catch {
  /* storage unavailable */
}

export const themePref = () => pref;

function apply(): void {
  const root = document.documentElement;
  if (pref === "system") delete root.dataset.theme;
  else root.dataset.theme = pref;
  root.dataset.effectiveTheme = pref === "system" ? (mq.matches ? "dark" : "light") : pref;
  emit("theme");
}

export function initTheme(): void {
  mq.addEventListener("change", () => pref === "system" && apply());
  apply();
}

export function setTheme(p: ThemePref): void {
  pref = p;
  try {
    localStorage.setItem(KEY, pref);
  } catch {
    /* ignore */
  }
  apply();
}

export function cycleTheme(): ThemePref {
  pref = ORDER[(ORDER.indexOf(pref) + 1) % ORDER.length];
  try {
    localStorage.setItem(KEY, pref);
  } catch {
    /* ignore */
  }
  apply();
  return pref;
}
