// Moving and free-transforming layers (or the selected pixels).
//
// The transform is kept as where the four corners of the original bounds have gone (a quad), so
// scale, rotate, skew, distort and perspective are all one homography. While it changes, the GPU
// draws the layer part of a live session through that matrix; Enter (or releasing a Move drag)
// commits it, and the engine resamples once from the original pixels.
import { engine, findLayer, type LayerInfo, type ResampleMethod } from '../../engine/bridge';
import { display } from '../../engine/display';
import { about, apply, convex, homography, multiply, rotate, translate, type Mat3, type Pt } from '../../engine/matrix';
import { app, docToViewport, viewportToDoc } from '../state.svelte';
import { t } from '../i18n/index.svelte';
import { message } from '../actions';

type Handle = { kind: 'corner'; i: number } | { kind: 'side'; i: number } | { kind: 'move' } | { kind: 'rotate' };

interface Active {
  doc: number;
  ids: number[];
  layer: LayerInfo | null;
  box: Pt[];
  quad: Pt[];
  session: number | null;
  /** Free transform (handles, Enter to commit) rather than a Move drag. */
  free: boolean;
}

const sub = (a: Pt, b: Pt): Pt => ({ x: a.x - b.x, y: a.y - b.y });
const add = (a: Pt, b: Pt): Pt => ({ x: a.x + b.x, y: a.y + b.y });
const len = (a: Pt) => Math.hypot(a.x, a.y);
const mid = (a: Pt, b: Pt): Pt => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });
const center = (q: Pt[]): Pt => ({ x: (q[0].x + q[1].x + q[2].x + q[3].x) / 4, y: (q[0].y + q[1].y + q[2].y + q[3].y) / 4 });
/** Reference point grid position (0–8, row by row) as fractions of the box. */
const refFraction = (r: number) => ({ fx: (r % 3) / 2, fy: Math.floor(r / 3) / 2 });

class TransformState {
  active = $state<Active | null>(null);
  method = $state<ResampleMethod>('bicubic');
  /** Reference point, 0–8 (4 = center). */
  ref = $state(4);
  hover = $state<Handle | null>(null);
  private drag: { h: Handle; start: Pt; quad: Pt[]; mods: { shift: boolean; alt: boolean; cmd: boolean } } | null = null;
  private starting: Promise<boolean> | null = null;
  private redraw: () => void = () => {};

  setRedraw(f: () => void) {
    this.redraw = f;
  }

  get busy() {
    return this.active != null || this.starting != null;
  }

  matrix(): Mat3 | null {
    const a = this.active;
    return a ? homography(a.box, a.quad) : null;
  }

  /** The reference point on the current quad. */
  refPoint(quad = this.active?.quad): Pt | null {
    const a = this.active;
    if (!a || !quad) return null;
    const m = homography(a.box, quad);
    if (!m) return center(quad);
    const { fx, fy } = refFraction(this.ref);
    return apply(m, { x: a.box[0].x + (a.box[1].x - a.box[0].x) * fx, y: a.box[0].y + (a.box[3].y - a.box[0].y) * fy });
  }

  /** Numbers for the options bar (meaningful while the quad is a parallelogram). */
  readout() {
    const a = this.active;
    if (!a) return null;
    const q = a.quad;
    const b = a.box;
    const r = this.refPoint()!;
    return {
      x: r.x,
      y: r.y,
      w: (len(sub(q[1], q[0])) / Math.max(1e-9, len(sub(b[1], b[0])))) * 100,
      h: (len(sub(q[3], q[0])) / Math.max(1e-9, len(sub(b[3], b[0])))) * 100,
      angle: (Math.atan2(q[1].y - q[0].y, q[1].x - q[0].x) * 180) / Math.PI,
    };
  }

  /** Starts moving or transforming the selected layers (or the selected pixels). */
  begin(free: boolean): Promise<boolean> {
    if (this.active) {
      if (free) this.active.free = true;
      return Promise.resolve(true);
    }
    if (this.starting) return this.starting;
    this.starting = this.start(free).finally(() => (this.starting = null));
    return this.starting;
  }

  private async start(free: boolean): Promise<boolean> {
    const d = app.active;
    if (!d) return false;
    const ids = d.selection.length ? [...d.selection] : [d.info.active];
    const layer = ids.length === 1 ? findLayer(d.info.layers, ids[0]) : null;
    if (ids.some((id) => findLayer(d.info.layers, id)?.locks.position)) {
      app.notify(t('xf.locked'), 'error');
      return false;
    }
    const b = await engine.transformBounds(d.info.id, ids).catch(() => null);
    if (!b || b.w <= 0 || b.h <= 0) {
      app.notify(t('xf.nothing'), 'info');
      return false;
    }
    const box = [
      { x: b.x, y: b.y },
      { x: b.x + b.w, y: b.y },
      { x: b.x + b.w, y: b.y + b.h },
      { x: b.x, y: b.y + b.h },
    ];
    let session: number | null = null;
    if (layer?.kind === 'pixel') session = await engine.liveBeginTransform(d.info.id, layer.id).catch(() => null);
    this.active = { doc: d.info.id, ids, layer, box, quad: box.map((p) => ({ ...p })), session, free };
    this.preview();
    return true;
  }

  private preview() {
    const a = this.active;
    const c = display.compositor;
    if (!a || !c || a.session == null || !a.layer) {
      this.redraw();
      return;
    }
    const m = this.matrix();
    if (m) c.setLive({ session: a.session, blend: a.layer.blend, alpha: a.layer.opacity * a.layer.fill, visible: a.layer.visible, transform: m });
    this.redraw();
  }

  setQuad(q: Pt[]) {
    if (!this.active || !convex(q)) return;
    this.active.quad = q;
    this.preview();
  }

  async commit() {
    const a = this.active;
    if (!a) return;
    const m = this.matrix();
    const unchanged = a.quad.every((p, i) => Math.abs(p.x - a.box[i].x) < 1e-6 && Math.abs(p.y - a.box[i].y) < 1e-6);
    this.active = null;
    this.drag = null;
    if (!m || unchanged) {
      this.end(a, false);
      return;
    }
    const moveOnly = a.quad.every((p, i) => Math.abs(p.x - a.box[i].x - (a.quad[0].x - a.box[0].x)) < 1e-9 && Math.abs(p.y - a.box[i].y - (a.quad[0].y - a.box[0].y)) < 1e-9);
    try {
      app.update(await engine.layerTransform(a.doc, a.ids, m, this.method, moveOnly ? 'history.movePixels' : 'history.transform'));
      this.end(a, true);
    } catch (e) {
      this.end(a, false);
      app.notify(t('err.generic', { msg: message(e) }), 'error');
    }
  }

  cancel() {
    const a = this.active;
    this.active = null;
    this.drag = null;
    if (a) this.end(a, false);
  }

  private end(a: Active, committed: boolean) {
    const c = display.compositor;
    const s = a.session;
    if (s != null) {
      if (committed) c?.endLive(() => void engine.liveEnd(s));
      else {
        c?.cancelLive();
        void engine.liveEnd(s);
      }
    }
    this.redraw();
  }

  // ---- pointer ----

  /** Which handle is under a viewport point. */
  hit(p: Pt): Handle | null {
    const a = this.active;
    const d = app.active;
    if (!a || !d) return null;
    const v = (q: Pt) => docToViewport(d.view, app.viewport, q.x, q.y);
    const vq = a.quad.map(v);
    if (a.free) {
      for (let i = 0; i < 4; i++) if (len(sub(vq[i], p)) <= 8) return { kind: 'corner', i };
      for (let i = 0; i < 4; i++) if (len(sub(mid(vq[i], vq[(i + 1) % 4]), p)) <= 8) return { kind: 'side', i };
    }
    const dp = viewportToDoc(d.view, app.viewport, p.x, p.y);
    if (inside(a.quad, dp)) return { kind: 'move' };
    return a.free ? { kind: 'rotate' } : { kind: 'move' };
  }

  cursor(): string | null {
    const h = this.drag?.h ?? this.hover;
    if (!h) return null;
    switch (h.kind) {
      case 'move':
        return 'move';
      case 'rotate':
        return 'alias';
      case 'corner':
        return h.i % 2 === 0 ? 'nwse-resize' : 'nesw-resize';
      case 'side':
        return h.i % 2 === 0 ? 'ns-resize' : 'ew-resize';
    }
  }

  down(e: PointerEvent, p: Pt) {
    const d = app.active;
    if (!d) return;
    const start = viewportToDoc(d.view, app.viewport, p.x, p.y);
    const mods = { shift: e.shiftKey, alt: e.altKey, cmd: e.metaKey || e.ctrlKey };
    if (!this.active) {
      // the Move tool: start a move from wherever the press was
      void this.begin(false).then((ok) => {
        if (ok && this.active) this.drag = { h: { kind: 'move' }, start, quad: this.active.quad.map((q) => ({ ...q })), mods };
      });
      return;
    }
    const h = this.hit(p) ?? { kind: 'move' };
    this.drag = { h, start, quad: this.active.quad.map((q) => ({ ...q })), mods };
  }

  move(e: PointerEvent, p: Pt) {
    const d = app.active;
    if (!d) return;
    if (!this.drag) {
      const h = this.active ? this.hit(p) : null;
      if (JSON.stringify(h) !== JSON.stringify(this.hover)) this.hover = h;
      return;
    }
    const a = this.active;
    if (!a) return;
    const cur = viewportToDoc(d.view, app.viewport, p.x, p.y);
    const q = this.dragQuad(this.drag, cur, { shift: e.shiftKey, alt: e.altKey, cmd: e.metaKey || e.ctrlKey });
    if (q) this.setQuad(q);
  }

  up() {
    const a = this.active;
    this.drag = null;
    // a Move drag commits on release; a free transform waits for Enter
    if (a && !a.free) void this.commit();
  }

  private dragQuad(dr: NonNullable<TransformState['drag']>, cur: Pt, m: { shift: boolean; alt: boolean; cmd: boolean }): Pt[] | null {
    const q0 = dr.quad;
    let delta = sub(cur, dr.start);
    const h = dr.h;
    if (h.kind === 'move') {
      if (m.shift) delta = Math.abs(delta.x) > Math.abs(delta.y) ? { x: delta.x, y: 0 } : { x: 0, y: delta.y };
      // whole pixels unless the image is zoomed far out, so plain moves stay exact
      delta = { x: Math.round(delta.x), y: Math.round(delta.y) };
      return q0.map((p) => add(p, delta));
    }
    if (h.kind === 'rotate') {
      const o = this.refPoint(q0)!;
      let ang = Math.atan2(cur.y - o.y, cur.x - o.x) - Math.atan2(dr.start.y - o.y, dr.start.x - o.x);
      if (m.shift) {
        const base = Math.atan2(q0[1].y - q0[0].y, q0[1].x - q0[0].x);
        const step = Math.PI / 12;
        ang = Math.round((base + ang) / step) * step - base;
      }
      return q0.map((p) => apply(about(o, rotate(ang)), p));
    }
    const theta = Math.atan2(q0[1].y - q0[0].y, q0[1].x - q0[0].x);
    const toLocal = rotate(-theta);
    const fromLocal = rotate(theta);
    if (h.kind === 'corner') {
      const i = h.i;
      if (m.cmd && m.alt && m.shift) {
        // perspective: this corner and its neighbour on the same edge move in mirror image
        const j = i === 0 ? 1 : i === 1 ? 0 : i === 2 ? 3 : 2;
        const dir = sub(q0[j], q0[i]);
        const u = { x: dir.x / len(dir), y: dir.y / len(dir) };
        const along = delta.x * u.x + delta.y * u.y;
        const q = q0.map((p) => ({ ...p }));
        q[i] = add(q0[i], { x: u.x * along, y: u.y * along });
        q[j] = add(q0[j], { x: -u.x * along, y: -u.y * along });
        return q;
      }
      if (m.cmd) {
        // distort: the corner goes where the pointer goes
        const q = q0.map((p) => ({ ...p }));
        q[i] = add(q0[i], delta);
        return q;
      }
      const o = m.alt ? this.refPoint(q0)! : q0[(i + 2) % 4];
      const d0 = apply(toLocal, sub(q0[i], o));
      const d1 = apply(toLocal, add(sub(q0[i], o), delta));
      let sx = Math.abs(d0.x) > 1e-9 ? d1.x / d0.x : 1;
      let sy = Math.abs(d0.y) > 1e-9 ? d1.y / d0.y : 1;
      // corners keep proportions; Shift frees them
      if (!m.shift) {
        const s = (d1.x * d0.x + d1.y * d0.y) / Math.max(1e-9, d0.x * d0.x + d0.y * d0.y);
        sx = sy = s;
      }
      const k = multiply(fromLocal, multiply([sx, 0, 0, 0, sy, 0, 0, 0, 1], toLocal));
      return q0.map((p) => add(o, apply(k, sub(p, o))));
    }
    // a side: scale across it, or skew along it with ⌘⇧
    const i = h.i;
    const a = q0[i];
    const b = q0[(i + 1) % 4];
    if (m.cmd) {
      const dir = sub(b, a);
      const u = { x: dir.x / len(dir), y: dir.y / len(dir) };
      const along = delta.x * u.x + delta.y * u.y;
      const shiftAlong = { x: u.x * along, y: u.y * along };
      const q = q0.map((p) => ({ ...p }));
      q[i] = add(a, shiftAlong);
      q[(i + 1) % 4] = add(b, shiftAlong);
      return q;
    }
    const o = m.alt ? this.refPoint(q0)! : mid(q0[(i + 2) % 4], q0[(i + 3) % 4]);
    const d0 = apply(toLocal, sub(mid(a, b), o));
    const d1 = apply(toLocal, add(sub(mid(a, b), o), delta));
    const vertical = i % 2 === 0; // top or bottom side: scale y
    let sx = 1;
    let sy = 1;
    if (vertical) sy = Math.abs(d0.y) > 1e-9 ? d1.y / d0.y : 1;
    else sx = Math.abs(d0.x) > 1e-9 ? d1.x / d0.x : 1;
    if (m.shift) sx = sy = vertical ? sy : sx;
    const k = multiply(fromLocal, multiply([sx, 0, 0, 0, sy, 0, 0, 0, 1], toLocal));
    return q0.map((p) => add(o, apply(k, sub(p, o))));
  }

  // ---- options bar and menu commands ----

  /** Sets position, size or angle from the options bar (straightens out any distortion). */
  setNumbers(n: { x?: number; y?: number; w?: number; h?: number; angle?: number }) {
    const a = this.active;
    const cur = this.readout();
    if (!a || !cur) return;
    const { fx, fy } = refFraction(this.ref);
    const rb = { x: a.box[0].x + (a.box[1].x - a.box[0].x) * fx, y: a.box[0].y + (a.box[3].y - a.box[0].y) * fy };
    const v = { ...cur, ...n };
    const m = multiply(translate(v.x, v.y), multiply(rotate((v.angle * Math.PI) / 180), multiply([v.w / 100, 0, 0, 0, v.h / 100, 0, 0, 0, 1], translate(-rb.x, -rb.y))));
    this.setQuad(a.box.map((p) => apply(m, p)));
  }

  /** Flips or turns the quad about the reference point. */
  turn(kind: 'flipH' | 'flipV' | 'cw90' | 'ccw90' | 'half') {
    const a = this.active;
    if (!a) return;
    const o = this.refPoint()!;
    const theta = Math.atan2(a.quad[1].y - a.quad[0].y, a.quad[1].x - a.quad[0].x);
    const local: Mat3 =
      kind === 'flipH' ? [-1, 0, 0, 0, 1, 0, 0, 0, 1] : kind === 'flipV' ? [1, 0, 0, 0, -1, 0, 0, 0, 1] : rotate(kind === 'cw90' ? Math.PI / 2 : kind === 'ccw90' ? -Math.PI / 2 : Math.PI);
    const k = about(o, multiply(rotate(theta), multiply(local, rotate(-theta))));
    // a flip mirrors the quad (its winding reverses, which the checks allow)
    a.quad = a.quad.map((p) => apply(k, p));
    this.preview();
  }

  draw(ctx: CanvasRenderingContext2D) {
    const a = this.active;
    const d = app.active;
    if (!a || !d || d.info.id !== a.doc) return;
    const v = (q: Pt) => docToViewport(d.view, app.viewport, q.x, q.y);
    const vq = a.quad.map(v);
    ctx.save();
    for (const [style, width] of [
      ['rgba(0,0,0,0.6)', 3],
      ['#ffffff', 1],
    ] as const) {
      ctx.beginPath();
      vq.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)));
      ctx.closePath();
      ctx.strokeStyle = style;
      ctx.lineWidth = width;
      ctx.stroke();
    }
    if (a.free) {
      const handles = [...vq, ...vq.map((p, i) => mid(p, vq[(i + 1) % 4]))];
      for (const h of handles) {
        ctx.fillStyle = '#ffffff';
        ctx.strokeStyle = 'rgba(0,0,0,0.7)';
        ctx.fillRect(Math.round(h.x) - 4, Math.round(h.y) - 4, 8, 8);
        ctx.strokeRect(Math.round(h.x) - 3.5, Math.round(h.y) - 3.5, 7, 7);
      }
      const r = v(this.refPoint()!);
      ctx.beginPath();
      ctx.arc(r.x, r.y, 5, 0, Math.PI * 2);
      ctx.moveTo(r.x - 8, r.y);
      ctx.lineTo(r.x + 8, r.y);
      ctx.moveTo(r.x, r.y - 8);
      ctx.lineTo(r.x, r.y + 8);
      ctx.strokeStyle = '#ffffff';
      ctx.lineWidth = 1;
      ctx.stroke();
    }
    ctx.restore();
  }
}

/** Point in a (convex) quad. */
function inside(q: Pt[], p: Pt): boolean {
  let sign = 0;
  for (let i = 0; i < 4; i++) {
    const a = q[i];
    const b = q[(i + 1) % 4];
    const z = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    const s = Math.sign(z);
    if (s === 0) continue;
    if (sign && s !== sign) return false;
    sign = s;
  }
  return true;
}

export const transform = new TransformState();
