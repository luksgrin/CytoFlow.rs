// Hand-built perceptual sequential colormaps (indigo -> teal -> green -> amber -> red).
// The dark variant starts brighter so sparse events stay visible on a dark background.

type Stop = [number, number, number, number]; // position, r, g, b

const LIGHT: Stop[] = [
  [0.0, 52, 40, 140],
  [0.18, 40, 92, 190],
  [0.36, 24, 150, 170],
  [0.54, 70, 180, 90],
  [0.72, 200, 200, 40],
  [0.86, 240, 140, 30],
  [1.0, 215, 40, 40],
];

const DARK: Stop[] = [
  [0.0, 92, 96, 220],
  [0.18, 60, 140, 235],
  [0.36, 40, 190, 200],
  [0.54, 90, 210, 110],
  [0.72, 225, 220, 60],
  [0.86, 250, 155, 45],
  [1.0, 245, 70, 60],
];

function build(stops: Stop[]): Uint8Array {
  const lut = new Uint8Array(256 * 3);
  for (let i = 0; i < 256; i++) {
    const t = i / 255;
    let k = 0;
    while (k < stops.length - 2 && t > stops[k + 1][0]) k++;
    const [p0, r0, g0, b0] = stops[k];
    const [p1, r1, g1, b1] = stops[k + 1];
    const u = Math.min(1, Math.max(0, (t - p0) / (p1 - p0)));
    // Smoothstep between stops avoids visible banding at the joints.
    const s = u * u * (3 - 2 * u);
    lut[i * 3] = Math.round(r0 + (r1 - r0) * s);
    lut[i * 3 + 1] = Math.round(g0 + (g1 - g0) * s);
    lut[i * 3 + 2] = Math.round(b0 + (b1 - b0) * s);
  }
  return lut;
}

const cache: Record<string, Uint8Array> = {};
export function colormap(dark: boolean): Uint8Array {
  const k = dark ? "dark" : "light";
  return (cache[k] ??= build(dark ? DARK : LIGHT));
}

export function lutCss(lut: Uint8Array, i: number, alpha = 1): string {
  const j = Math.max(0, Math.min(255, Math.round(i))) * 3;
  return `rgba(${lut[j]},${lut[j + 1]},${lut[j + 2]},${alpha})`;
}

/** Distinct colours for gates without an explicit colour. */
export const GATE_COLORS = [
  "#2563eb", "#dc2626", "#16a34a", "#d97706", "#9333ea", "#0891b2", "#db2777", "#65a30d", "#4b5563", "#ea580c",
];
