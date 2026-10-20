// Vector shapes and paths in the interface: the same types as mosamem-core's vector.rs, the shape →
// path math (for previews while drawing and for the Pen and Direct Selection tools), and drawing a
// path on a canvas. The engine draws the real pixels.
import type { GradientStop } from './adjust';

export type Pt = [number, number];

export interface Knot {
  p: Pt;
  /** Handle toward the previous knot. */
  i: Pt;
  /** Handle toward the next knot. */
  o: Pt;
  smooth: boolean;
}
export interface SubPath {
  knots: Knot[];
  closed: boolean;
}
export interface Path {
  subs: SubPath[];
}

export type Shape =
  | { kind: 'rect'; x: number; y: number; w: number; h: number; radius: number }
  | { kind: 'ellipse'; x: number; y: number; w: number; h: number }
  | { kind: 'polygon'; cx: number; cy: number; r: number; sides: number; angle: number }
  | { kind: 'star'; cx: number; cy: number; r: number; inner: number; points: number; angle: number }
  | { kind: 'line'; a: Pt; b: Pt; arrowStart: boolean; arrowEnd: boolean }
  | { kind: 'free'; path: Path };
export type ShapeKind = Shape['kind'];

export type Fill = { kind: 'solid'; color: [number, number, number, number] } | { kind: 'linear'; stops: GradientStop[]; angle: number };
export type StrokeAlign = 'inside' | 'center' | 'outside';
export interface Stroke {
  color: [number, number, number, number];
  width: number;
  align: StrokeAlign;
  cap: 'butt' | 'round' | 'square';
  join: 'miter' | 'round' | 'bevel';
  /** Dash and gap lengths as multiples of the width (empty: solid). */
  dash: number[];
}
export interface ShapePaint {
  fill: Fill | null;
  stroke: Stroke | null;
}
export interface ShapeData {
  shape: Shape;
  paint: ShapePaint;
}

export const KAPPA = 0.5522847498307934;

const corner = (p: Pt): Knot => ({ p, i: p, o: p, smooth: false });

function rectPath(x: number, y: number, w: number, h: number, radius: number): Path {
  const [x0, y0, x1, y1] = [Math.min(x, x + w), Math.min(y, y + h), Math.max(x, x + w), Math.max(y, y + h)];
  const r = Math.max(0, Math.min(radius, (x1 - x0) / 2, (y1 - y0) / 2));
  if (r <= 0) return { subs: [{ knots: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map((p) => corner(p as Pt)), closed: true }] };
  const k = r * KAPPA;
  const s = (p: Pt, i: Pt, o: Pt): Knot => ({ p, i, o, smooth: true });
  return {
    subs: [
      {
        closed: true,
        knots: [
          s([x0 + r, y0], [x0 + r - k, y0], [x0 + r, y0]),
          s([x1 - r, y0], [x1 - r, y0], [x1 - r + k, y0]),
          s([x1, y0 + r], [x1, y0 + r - k], [x1, y0 + r]),
          s([x1, y1 - r], [x1, y1 - r], [x1, y1 - r + k]),
          s([x1 - r, y1], [x1 - r + k, y1], [x1 - r, y1]),
          s([x0 + r, y1], [x0 + r, y1], [x0 + r - k, y1]),
          s([x0, y1 - r], [x0, y1 - r + k], [x0, y1 - r]),
          s([x0, y0 + r], [x0, y0 + r], [x0, y0 + r - k]),
        ],
      },
    ],
  };
}

function ellipsePath(x: number, y: number, w: number, h: number): Path {
  const [rx, ry] = [Math.abs(w) / 2, Math.abs(h) / 2];
  const [cx, cy] = [x + w / 2, y + h / 2];
  const [kx, ky] = [rx * KAPPA, ry * KAPPA];
  const s = (p: Pt, i: Pt, o: Pt): Knot => ({ p, i, o, smooth: true });
  return {
    subs: [
      {
        closed: true,
        knots: [
          s([cx, cy - ry], [cx - kx, cy - ry], [cx + kx, cy - ry]),
          s([cx + rx, cy], [cx + rx, cy - ky], [cx + rx, cy + ky]),
          s([cx, cy + ry], [cx + kx, cy + ry], [cx - kx, cy + ry]),
          s([cx - rx, cy], [cx - rx, cy + ky], [cx - rx, cy - ky]),
        ],
      },
    ],
  };
}

function ring(cx: number, cy: number, radii: number[], n: number, angle: number): Path {
  const count = n * radii.length;
  const knots = Array.from({ length: count }, (_, i) => {
    const a = ((angle - 90) * Math.PI) / 180 + (i * Math.PI * 2) / count;
    const r = radii[i % radii.length];
    return corner([cx + r * Math.cos(a), cy + r * Math.sin(a)]);
  });
  return { subs: [{ knots, closed: true }] };
}

/** The outline of a shape (for a line: the line itself). Same math as vector.rs. */
export function shapePath(s: Shape): Path {
  switch (s.kind) {
    case 'rect':
      return rectPath(s.x, s.y, s.w, s.h, s.radius);
    case 'ellipse':
      return ellipsePath(s.x, s.y, s.w, s.h);
    case 'polygon':
      return ring(s.cx, s.cy, [s.r], Math.max(3, s.sides), s.angle);
    case 'star':
      return ring(s.cx, s.cy, [s.r, s.r * Math.min(1, Math.max(0.01, s.inner))], Math.max(3, s.points), s.angle);
    case 'line':
      return { subs: [{ knots: [corner(s.a), corner(s.b)], closed: false }] };
    case 'free':
      return s.path;
  }
}

/** The cubic segments of a subpath: [from, control 1, control 2, to]. */
export function segments(s: SubPath): [Pt, Pt, Pt, Pt][] {
  const n = s.knots.length;
  if (n < 2) return [];
  const count = s.closed ? n : n - 1;
  return Array.from({ length: count }, (_, k) => {
    const a = s.knots[k];
    const b = s.knots[(k + 1) % n];
    return [a.p, a.o, b.i, b.p];
  });
}

/** Traces a path on a 2D canvas, mapping document points with `to`. */
export function tracePath(ctx: CanvasRenderingContext2D, path: Path, to: (p: Pt) => Pt) {
  ctx.beginPath();
  for (const s of path.subs) {
    if (!s.knots.length) continue;
    const f = to(s.knots[0].p);
    ctx.moveTo(f[0], f[1]);
    for (const [, c1, c2, b] of segments(s)) {
      const [p1, p2, p3] = [to(c1), to(c2), to(b)];
      ctx.bezierCurveTo(p1[0], p1[1], p2[0], p2[1], p3[0], p3[1]);
    }
    if (s.closed) ctx.closePath();
  }
}

/** The shape a drag from `a` to `b` draws (Shift keeps it square / at 45°, Alt draws from the center). */
export function dragShape(kind: Exclude<ShapeKind, 'free'>, a: Pt, b: Pt, opts: { square: boolean; center: boolean; radius: number; sides: number; inner: number; points: number; arrowStart: boolean; arrowEnd: boolean }): Shape {
  let [dx, dy] = [b[0] - a[0], b[1] - a[1]];
  if (kind === 'line') {
    if (opts.square) {
      const ang = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
      const len = Math.hypot(dx, dy);
      [dx, dy] = [Math.cos(ang) * len, Math.sin(ang) * len];
    }
    const start: Pt = opts.center ? [a[0] - dx, a[1] - dy] : a;
    return { kind: 'line', a: start, b: [a[0] + dx, a[1] + dy], arrowStart: opts.arrowStart, arrowEnd: opts.arrowEnd };
  }
  if (kind === 'polygon' || kind === 'star') {
    // from the center out; the drag's direction turns it
    const r = Math.hypot(dx, dy);
    let angle = (Math.atan2(dy, dx) * 180) / Math.PI + 90;
    if (opts.square) angle = Math.round(angle / 15) * 15;
    return kind === 'polygon'
      ? { kind, cx: a[0], cy: a[1], r, sides: opts.sides, angle }
      : { kind, cx: a[0], cy: a[1], r, inner: opts.inner, points: opts.points, angle };
  }
  if (opts.square) {
    const m = Math.max(Math.abs(dx), Math.abs(dy));
    [dx, dy] = [Math.sign(dx || 1) * m, Math.sign(dy || 1) * m];
  }
  const [x, y, w, h] = opts.center ? [a[0] - dx, a[1] - dy, 2 * dx, 2 * dy] : [a[0], a[1], dx, dy];
  // positive width and height
  const box = { x: Math.min(x, x + w), y: Math.min(y, y + h), w: Math.abs(w), h: Math.abs(h) };
  return kind === 'rect' ? { kind, ...box, radius: opts.radius } : { kind: 'ellipse', ...box };
}

const lerp = (a: Pt, b: Pt, t: number): Pt => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];

/** A point on a cubic segment. */
export function pointAt(c: [Pt, Pt, Pt, Pt], t: number): Pt {
  const u = 1 - t;
  const [a, b, d, e] = [u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t];
  return [a * c[0][0] + b * c[1][0] + d * c[2][0] + e * c[3][0], a * c[0][1] + b * c[1][1] + d * c[2][1] + e * c[3][1]];
}

/** The segment nearest to `p` within `tol`: subpath, segment, t and distance (as vector.rs). */
export function nearestSegment(path: Path, p: Pt, tol: number): { sub: number; seg: number; t: number; dist: number } | null {
  let best: { sub: number; seg: number; t: number; dist: number } | null = null;
  const dist = (q: Pt) => Math.hypot(q[0] - p[0], q[1] - p[1]);
  path.subs.forEach((s, si) =>
    segments(s).forEach((c, k) => {
      // coarse sampling, then refine around the best sample
      let [bt, bd] = [0, Infinity];
      for (let i = 0; i <= 64; i++) {
        const d = dist(pointAt(c, i / 64));
        if (d < bd) [bd, bt] = [d, i / 64];
      }
      let step = 1 / 64;
      for (let r = 0; r < 20; r++) {
        step /= 2;
        for (const t of [bt - step, bt + step]) {
          if (t < 0 || t > 1) continue;
          const d = dist(pointAt(c, t));
          if (d < bd) [bd, bt] = [d, t];
        }
      }
      if (bd <= tol && (!best || bd < best.dist)) best = { sub: si, seg: k, t: bt, dist: bd };
    }),
  );
  return best;
}

/** Splits a segment at t with a new knot (the curve keeps its shape); returns the knot's index. */
export function insertKnot(path: Path, sub: number, seg: number, t: number): number {
  const s = path.subs[sub];
  const n = s.knots.length;
  const c = segments(s)[seg];
  const [a, b, d] = [lerp(c[0], c[1], t), lerp(c[1], c[2], t), lerp(c[2], c[3], t)];
  const [e, f] = [lerp(a, b, t), lerp(b, d, t)];
  const straight = eq(c[0], c[1]) && eq(c[2], c[3]);
  s.knots[seg].o = a;
  s.knots[(seg + 1) % n].i = d;
  s.knots.splice(seg + 1, 0, { p: lerp(e, f, t), i: e, o: f, smooth: !straight });
  return seg + 1;
}

const eq = (a: Pt, b: Pt) => a[0] === b[0] && a[1] === b[1];

/** A deep copy (paths are edited in place by the Pen and Direct Selection tools). */
export const clonePath = (p: Path): Path => JSON.parse(JSON.stringify(p));
