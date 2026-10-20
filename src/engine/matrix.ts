// 3×3 matrices (row-major, acting on (x, y, 1)), as in lawha_core::resample.

export type Mat3 = number[];
export type Pt = { x: number; y: number };

export const IDENTITY: Mat3 = [1, 0, 0, 0, 1, 0, 0, 0, 1];

export function apply(m: Mat3, p: Pt): Pt {
  const w = m[6] * p.x + m[7] * p.y + m[8];
  return { x: (m[0] * p.x + m[1] * p.y + m[2]) / w, y: (m[3] * p.x + m[4] * p.y + m[5]) / w };
}

export function multiply(a: Mat3, b: Mat3): Mat3 {
  const r = new Array<number>(9).fill(0);
  for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) for (let k = 0; k < 3; k++) r[i * 3 + j] += a[i * 3 + k] * b[k * 3 + j];
  return r;
}

export const translate = (x: number, y: number): Mat3 => [1, 0, x, 0, 1, y, 0, 0, 1];
export const scale = (sx: number, sy: number): Mat3 => [sx, 0, 0, 0, sy, 0, 0, 0, 1];
export function rotate(rad: number): Mat3 {
  const c = Math.cos(rad);
  const s = Math.sin(rad);
  return [c, -s, 0, s, c, 0, 0, 0, 1];
}

/** `m` applied around `o`. */
export const about = (o: Pt, m: Mat3): Mat3 => multiply(translate(o.x, o.y), multiply(m, translate(-o.x, -o.y)));

/** The projective transform taking the corners `from` to `to` (null when degenerate). */
export function homography(from: Pt[], to: Pt[]): Mat3 | null {
  const a: number[][] = [];
  for (let k = 0; k < 4; k++) {
    const { x, y } = from[k];
    const { x: u, y: v } = to[k];
    a.push([x, y, 1, 0, 0, 0, -u * x, -u * y, u]);
    a.push([0, 0, 0, x, y, 1, -v * x, -v * y, v]);
  }
  for (let col = 0; col < 8; col++) {
    let p = col;
    for (let i = col + 1; i < 8; i++) if (Math.abs(a[i][col]) > Math.abs(a[p][col])) p = i;
    if (Math.abs(a[p][col]) < 1e-12) return null;
    [a[col], a[p]] = [a[p], a[col]];
    for (let r = 0; r < 8; r++) {
      if (r === col) continue;
      const f = a[r][col] / a[col][col];
      for (let c = col; c < 9; c++) a[r][c] -= f * a[col][c];
    }
  }
  const s = a.map((row, i) => row[8] / row[i]);
  return [s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], 1];
}

/** True when the four points make a convex quadrilateral in a consistent turning direction. */
export function convex(q: Pt[]): boolean {
  let sign = 0;
  for (let i = 0; i < 4; i++) {
    const a = q[i];
    const b = q[(i + 1) % 4];
    const c = q[(i + 2) % 4];
    const z = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
    if (Math.abs(z) < 1e-9) return false;
    const s = Math.sign(z);
    if (sign && s !== sign) return false;
    sign = s;
  }
  return true;
}
