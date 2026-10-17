// Gaussian smoothing, probability levels and marching squares on a bins x bins grid.

export function smooth(counts: ArrayLike<number>, bins: number, sigma = 1.6): Float32Array {
  const r = Math.ceil(sigma * 3);
  const k: number[] = [];
  let ks = 0;
  for (let i = -r; i <= r; i++) {
    const v = Math.exp(-(i * i) / (2 * sigma * sigma));
    k.push(v);
    ks += v;
  }
  for (let i = 0; i < k.length; i++) k[i] /= ks;
  const tmp = new Float32Array(bins * bins);
  const out = new Float32Array(bins * bins);
  for (let y = 0; y < bins; y++)
    for (let x = 0; x < bins; x++) {
      let s = 0;
      for (let i = -r; i <= r; i++) {
        const xx = x + i;
        if (xx >= 0 && xx < bins) s += counts[y * bins + xx] * k[i + r];
      }
      tmp[y * bins + x] = s;
    }
  for (let y = 0; y < bins; y++)
    for (let x = 0; x < bins; x++) {
      let s = 0;
      for (let i = -r; i <= r; i++) {
        const yy = y + i;
        if (yy >= 0 && yy < bins) s += tmp[yy * bins + x] * k[i + r];
      }
      out[y * bins + x] = s;
    }
  return out;
}

/**
 * Equal-probability levels: level i is the density threshold whose super-level set
 * contains `fractions[i]` of all events.
 */
export function probabilityLevels(grid: Float32Array, fractions: number[]): number[] {
  const vals = Array.from(grid).filter((v) => v > 0).sort((a, b) => b - a);
  const total = vals.reduce((a, b) => a + b, 0);
  if (!total) return [];
  const out: number[] = [];
  let acc = 0;
  let fi = 0;
  const fr = [...fractions].sort((a, b) => a - b);
  for (const v of vals) {
    acc += v;
    while (fi < fr.length && acc >= fr[fi] * total) {
      out.push(v);
      fi++;
    }
    if (fi >= fr.length) break;
  }
  return Array.from(new Set(out)).sort((a, b) => a - b); // ascending density
}

/** Marching squares; returns flat segment list [x1,y1,x2,y2,...] in bin-centre index units. */
export function marchingSquares(grid: Float32Array, bins: number, level: number): Float32Array {
  const seg: number[] = [];
  const at = (x: number, y: number) => grid[y * bins + x];
  const lerp = (a: number, b: number) => (a === b ? 0.5 : (level - a) / (b - a));
  for (let y = 0; y < bins - 1; y++) {
    for (let x = 0; x < bins - 1; x++) {
      const v0 = at(x, y), v1 = at(x + 1, y), v2 = at(x + 1, y + 1), v3 = at(x, y + 1);
      const c = (v0 >= level ? 1 : 0) | (v1 >= level ? 2 : 0) | (v2 >= level ? 4 : 0) | (v3 >= level ? 8 : 0);
      if (c === 0 || c === 15) continue;
      // Edge points: bottom (0-1), right (1-2), top (3-2), left (0-3)
      const B: [number, number] = [x + lerp(v0, v1), y];
      const R: [number, number] = [x + 1, y + lerp(v1, v2)];
      const T: [number, number] = [x + lerp(v3, v2), y + 1];
      const L: [number, number] = [x, y + lerp(v0, v3)];
      const add = (p: [number, number], q: [number, number]) => seg.push(p[0], p[1], q[0], q[1]);
      const centre = (v0 + v1 + v2 + v3) / 4 >= level;
      switch (c) {
        case 1: case 14: add(L, B); break;
        case 2: case 13: add(B, R); break;
        case 3: case 12: add(L, R); break;
        case 4: case 11: add(R, T); break;
        case 6: case 9: add(B, T); break;
        case 7: case 8: add(L, T); break;
        case 5: if (centre) { add(L, T); add(B, R); } else { add(L, B); add(R, T); } break;
        case 10: if (centre) { add(L, B); add(R, T); } else { add(L, T); add(B, R); } break;
      }
    }
  }
  return Float32Array.from(seg);
}
