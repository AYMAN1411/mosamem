// Monotone cubic interpolation (Fritsch–Carlson): the same curve the engine builds for Curves
// (`Spline` in mosamem-core/src/adjust.rs), so the panel draws exactly what is applied.

/** Points in 0–255 (input, output). Returns f(x) for x in 0–255, flat beyond the end points. */
export function monotoneSpline(points: [number, number][]): (x: number) => number {
  const pts = points
    .map(([x, y]) => [Math.max(0, Math.min(255, x)) / 255, Math.max(0, Math.min(255, y)) / 255] as [number, number])
    .sort((a, b) => a[0] - b[0])
    .filter((p, i, a) => i === 0 || Math.abs(p[0] - a[i - 1][0]) >= 1e-6);
  if (!pts.length) pts.push([0, 0], [1, 1]);
  const xs = pts.map((p) => p[0]);
  const ys = pts.map((p) => p[1]);
  const n = xs.length;
  const m = new Array<number>(n).fill(0);
  if (n >= 2) {
    const d = xs.slice(0, -1).map((_, k) => (ys[k + 1] - ys[k]) / (xs[k + 1] - xs[k]));
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for (let k = 1; k < n - 1; k++) m[k] = d[k - 1] * d[k] <= 0 ? 0 : (d[k - 1] + d[k]) / 2;
    for (let k = 0; k < n - 1; k++) {
      if (d[k] === 0) {
        m[k] = 0;
        m[k + 1] = 0;
        continue;
      }
      const a = m[k] / d[k];
      const b = m[k + 1] / d[k];
      const s = a * a + b * b;
      if (s > 9) {
        const t = 3 / Math.sqrt(s);
        m[k] = t * a * d[k];
        m[k + 1] = t * b * d[k];
      }
    }
  }
  return (x255: number) => {
    const v = x255 / 255;
    if (n === 1 || v <= xs[0]) return ys[0] * 255;
    if (v >= xs[n - 1]) return ys[n - 1] * 255;
    let k = 0;
    while (k < n - 2 && xs[k + 1] <= v) k++;
    const h = xs[k + 1] - xs[k];
    const t = (v - xs[k]) / h;
    const t2 = t * t;
    const t3 = t2 * t;
    const y = (2 * t3 - 3 * t2 + 1) * ys[k] + (t3 - 2 * t2 + t) * h * m[k] + (-2 * t3 + 3 * t2) * ys[k + 1] + (t3 - t2) * h * m[k + 1];
    return Math.max(0, Math.min(1, y)) * 255;
  };
}
