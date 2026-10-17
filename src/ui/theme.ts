// Light / dark / system theme handling.
import { emit } from "../state";

export type ThemePref = "system" | "light" | "dark";
const KEY = "cytoflow.theme";
const mq = window.matchMedia("(prefers-color-scheme: dark)");

let pref: ThemePref = "system";
try {
  const v = localStorage.getItem(KEY);
  if (v === "light" || v === "dark" || v === "system") pref = v;
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

export function cycleTheme(): ThemePref {
  pref = pref === "system" ? "light" : pref === "light" ? "dark" : "system";
  try {
    localStorage.setItem(KEY, pref);
  } catch {
    /* ignore */
  }
  apply();
  return pref;
}
