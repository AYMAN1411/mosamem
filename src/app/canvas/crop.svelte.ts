// The Crop tool: a rectangle (with its own angle, for straightening) over the canvas. Drag the
// handles to resize, inside to move, outside to turn it; Enter crops.
import { engine } from '../../engine/bridge';
import type { Pt } from '../../engine/matrix';
import { app, docToViewport, viewportToDoc } from '../state.svelte';
import { t } from '../i18n/index.svelte';
import { message } from '../actions';

/** Center, size and angle (degrees) in document px. */
interface Rect {
  cx: number;
  cy: number;
  w: number;
  h: number;
  angle: number;
}

type Handle = { kind: 'corner'; i: number } | { kind: 'side'; i: number } | { kind: 'move' } | { kind: 'rotate' };

export const RATIOS: { id: string; r: [number, number] | null }[] = [
  { id: 'free', r: null },
  { id: 'original', r: null },
  { id: '1:1', r: [1, 1] },
  { id: '4:5', r: [4, 5] },
  { id: '5:4', r: [5, 4] },
  { id: '2:3', r: [2, 3] },
  { id: '3:2', r: [3, 2] },
  { id: '16:9', r: [16, 9] },
  { id: '9:16', r: [9, 16] },
];

const rad = (d: number) => (d * Math.PI) / 180;

class CropState {
  rect = $state<Rect | null>(null);
  doc = $state<number | null>(null);
  ratio = $state('free');
  deleteCropped = $state(true);
  straightening = $state(false);
  hover = $state<Handle | null>(null);
  private drag: { h: Handle; start: Pt; rect: Rect; line?: Pt } | null = null;
  private line: [Pt, Pt] | null = null;
  private redraw: () => void = () => {};

  setRedraw(f: () => void) {
    this.redraw = f;
  }

  /** The crop covers the whole canvas again. */
  reset() {
    const d = app.active;
    this.doc = d?.info.id ?? null;
    this.rect = d ? { cx: d.info.width / 2, cy: d.info.height / 2, w: d.info.width, h: d.info.height, angle: 0 } : null;
    this.straightening = false;
    this.redraw();
  }

  ensure() {
    const d = app.active;
    if (!d) return;
    if (this.doc !== d.info.id || !this.rect) this.reset();
  }

  aspect(): number | null {
    const d = app.active;
    if (this.ratio === 'free' || !d) return null;
    if (this.ratio === 'original') return d.info.width / d.info.height;
    const r = RATIOS.find((x) => x.id === this.ratio)?.r;
    return r ? r[0] / r[1] : null;
  }

  /** Applies a ratio to the current rectangle, keeping its center and area roughly. */
  setRatio(id: string) {
    this.ratio = id;
    const a = this.aspect();
    const r = this.rect;
    if (!r || !a) return;
    const area = r.w * r.h;
    let w = Math.sqrt(area * a);
    let h = w / a;
    const d = app.active!;
    const k = Math.min(1, d.info.width / w, d.info.height / h);
    w *= k;
    h *= k;
    this.rect = { ...r, w, h };
    this.redraw();
  }

  setSize(w: number, h: number) {
    if (!this.rect || !(w > 0) || !(h > 0)) return;
    this.rect = { ...this.rect, w, h };
    this.redraw();
  }

  setAngle(a: number) {
    if (!this.rect) return;
    this.rect = this.fitInside({ ...this.rect, angle: a });
    this.redraw();
  }

  /** A turned crop shrinks (same center and proportions) until its corners are inside the canvas,
   *  so straightening doesn't bring in empty corners. */
  private fitInside(r: Rect): Rect {
    const d = app.active;
    if (!d || Math.abs(r.angle) < 1e-6) return r;
    const W = d.info.width;
    const H = d.info.height;
    const cx = Math.min(Math.max(r.cx, 1), W - 1);
    const cy = Math.min(Math.max(r.cy, 1), H - 1);
    let k = 1;
    for (const p of this.corners({ ...r, cx: 0, cy: 0 })) {
      if (p.x > 0) k = Math.min(k, (W - cx) / p.x);
      if (p.x < 0) k = Math.min(k, -cx / p.x);
      if (p.y > 0) k = Math.min(k, (H - cy) / p.y);
      if (p.y < 0) k = Math.min(k, -cy / p.y);
    }
    return { cx, cy, w: r.w * k, h: r.h * k, angle: r.angle };
  }

  corners(r = this.rect): Pt[] {
    if (!r) return [];
    const c = Math.cos(rad(r.angle));
    const s = Math.sin(rad(r.angle));
    return [
      [-1, -1],
      [1, -1],
      [1, 1],
      [-1, 1],
    ].map(([u, v]) => {
      const x = (u * r.w) / 2;
      const y = (v * r.h) / 2;
      return { x: r.cx + c * x - s * y, y: r.cy + s * x + c * y };
    });
  }

  /** Rectangle-local coordinates of a document point (origin at the center, unrotated). */
  private local(r: Rect, p: Pt): Pt {
    const c = Math.cos(rad(-r.angle));
    const s = Math.sin(rad(-r.angle));
    const x = p.x - r.cx;
    const y = p.y - r.cy;
    return { x: c * x - s * y, y: s * x + c * y };
  }

  hit(p: Pt): Handle | null {
    const d = app.active;
    const r = this.rect;
    if (!d || !r) return null;
    const vq = this.corners().map((q) => docToViewport(d.view, app.viewport, q.x, q.y));
    for (let i = 0; i < 4; i++) if (Math.hypot(vq[i].x - p.x, vq[i].y - p.y) <= 9) return { kind: 'corner', i };
    for (let i = 0; i < 4; i++) {
      const m = { x: (vq[i].x + vq[(i + 1) % 4].x) / 2, y: (vq[i].y + vq[(i + 1) % 4].y) / 2 };
      if (Math.hypot(m.x - p.x, m.y - p.y) <= 9) return { kind: 'side', i };
    }
    const l = this.local(r, viewportToDoc(d.view, app.viewport, p.x, p.y));
    return Math.abs(l.x) <= r.w / 2 && Math.abs(l.y) <= r.h / 2 ? { kind: 'move' } : { kind: 'rotate' };
  }

  cursor(): string | null {
    if (this.straightening) return 'crosshair';
    const h = this.drag?.h ?? this.hover;
    if (!h) return null;
    if (h.kind === 'move') return 'move';
    if (h.kind === 'rotate') return 'alias';
    if (h.kind === 'corner') return h.i % 2 === 0 ? 'nwse-resize' : 'nesw-resize';
    return h.i % 2 === 0 ? 'ns-resize' : 'ew-resize';
  }

  down(e: PointerEvent, p: Pt) {
    this.ensure();
    const d = app.active;
    const r = this.rect;
    if (!d || !r) return;
    const dp = viewportToDoc(d.view, app.viewport, p.x, p.y);
    if (this.straightening) {
      this.line = [dp, dp];
      this.drag = { h: { kind: 'rotate' }, start: dp, rect: { ...r }, line: dp };
      return;
    }
    this.drag = { h: this.hit(p) ?? { kind: 'move' }, start: dp, rect: { ...r } };
  }

  move(e: PointerEvent, p: Pt) {
    const d = app.active;
    if (!d) return;
    if (!this.drag) {
      const h = this.hit(p);
      if (JSON.stringify(h) !== JSON.stringify(this.hover)) this.hover = h;
      return;
    }
    const dp = viewportToDoc(d.view, app.viewport, p.x, p.y);
    const dr = this.drag;
    const r0 = dr.rect;
    if (this.straightening && this.line) {
      this.line = [this.line[0], dp];
      this.redraw();
      return;
    }
    if (dr.h.kind === 'move') {
      this.rect = { ...r0, cx: r0.cx + dp.x - dr.start.x, cy: r0.cy + dp.y - dr.start.y };
    } else if (dr.h.kind === 'rotate') {
      let a = r0.angle + ((Math.atan2(dp.y - r0.cy, dp.x - r0.cx) - Math.atan2(dr.start.y - r0.cy, dr.start.x - r0.cx)) * 180) / Math.PI;
      if (e.shiftKey) a = Math.round(a / 15) * 15;
      this.rect = { ...r0, angle: a };
    } else {
      // resize in the rectangle's own frame, the opposite edge or corner staying put
      const l = this.local(r0, dp);
      const s = this.local(r0, dr.start);
      const dx = l.x - s.x;
      const dy = l.y - s.y;
      let x0 = -r0.w / 2;
      let x1 = r0.w / 2;
      let y0 = -r0.h / 2;
      let y1 = r0.h / 2;
      const i = dr.h.i;
      if (dr.h.kind === 'corner') {
        if (i === 0 || i === 3) x0 += dx;
        else x1 += dx;
        if (i === 0 || i === 1) y0 += dy;
        else y1 += dy;
      } else if (i === 0) y0 += dy;
      else if (i === 1) x1 += dx;
      else if (i === 2) y1 += dy;
      else x0 += dx;
      let w = Math.max(1, x1 - x0);
      let h = Math.max(1, y1 - y0);
      const a = this.aspect() ?? (e.shiftKey ? r0.w / r0.h : null);
      if (a) {
        if (dr.h.kind === 'side' && i % 2 === 1) h = w / a;
        else if (dr.h.kind === 'side') w = h * a;
        else if (w / h > a) h = w / a;
        else w = h * a;
        // keep the anchored side in place
        if (dr.h.kind === 'corner') {
          if (i === 0 || i === 3) x0 = x1 - w;
          else x1 = x0 + w;
          if (i === 0 || i === 1) y0 = y1 - h;
          else y1 = y0 + h;
        } else if (i % 2 === 1) {
          const cy = (y0 + y1) / 2;
          y0 = cy - h / 2;
          y1 = cy + h / 2;
        } else {
          const cx = (x0 + x1) / 2;
          x0 = cx - w / 2;
          x1 = cx + w / 2;
        }
      }
      const lc = { x: (x0 + x1) / 2, y: (y0 + y1) / 2 };
      const c = Math.cos(rad(r0.angle));
      const sn = Math.sin(rad(r0.angle));
      this.rect = { cx: r0.cx + c * lc.x - sn * lc.y, cy: r0.cy + sn * lc.x + c * lc.y, w: x1 - x0, h: y1 - y0, angle: r0.angle };
    }
    this.redraw();
  }

  up() {
    if (this.straightening && this.line) {
      const [a, b] = this.line;
      this.line = null;
      this.straightening = false;
      if (Math.hypot(b.x - a.x, b.y - a.y) > 4 && this.rect) {
        // make the drawn line level (or upright, if it was closer to vertical)
        let deg = (Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI;
        if (deg > 90) deg -= 180;
        if (deg < -90) deg += 180;
        if (Math.abs(deg) > 45) deg = deg > 0 ? deg - 90 : deg + 90;
        // the crop follows the tilt; cropping then turns the picture so the line comes out level
        this.rect = this.fitInside({ ...this.rect, angle: deg });
      }
    }
    if (this.drag?.h.kind === 'rotate' && this.rect && !this.straightening) this.rect = this.fitInside(this.rect);
    this.drag = null;
    this.redraw();
  }

  async commit() {
    const d = app.active;
    const r = this.rect;
    if (!d || !r) return;
    const w = Math.round(r.w);
    const h = Math.round(r.h);
    const unchanged = Math.abs(r.angle) < 1e-6 && Math.round(r.cx - r.w / 2) === 0 && Math.round(r.cy - r.h / 2) === 0 && w === d.info.width && h === d.info.height;
    if (unchanged) return;
    try {
      app.update(await engine.crop(d.info.id, r.cx - w / 2, r.cy - h / 2, w, h, r.angle, this.deleteCropped, 'bicubic'));
      this.reset();
      app.fit();
    } catch (e) {
      app.notify(t('err.generic', { msg: message(e) }), 'error');
    }
  }

  draw(ctx: CanvasRenderingContext2D) {
    const d = app.active;
    const r = this.rect;
    if (!d || !r || this.doc !== d.info.id) return;
    const v = (q: Pt) => docToViewport(d.view, app.viewport, q.x, q.y);
    const vq = this.corners().map(v);
    ctx.save();
    // shade outside the crop
    ctx.beginPath();
    ctx.rect(0, 0, app.viewport.w, app.viewport.h);
    vq.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)));
    ctx.closePath();
    ctx.fillStyle = 'rgba(0,0,0,0.55)';
    ctx.fill('evenodd');
    // rule of thirds
    const lerp = (a: Pt, b: Pt, k: number) => ({ x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k });
    ctx.beginPath();
    for (const k of [1 / 3, 2 / 3]) {
      const a = lerp(vq[0], vq[1], k);
      const b = lerp(vq[3], vq[2], k);
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      const c = lerp(vq[0], vq[3], k);
      const e = lerp(vq[1], vq[2], k);
      ctx.moveTo(c.x, c.y);
      ctx.lineTo(e.x, e.y);
    }
    ctx.strokeStyle = 'rgba(255,255,255,0.45)';
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.beginPath();
    vq.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)));
    ctx.closePath();
    ctx.strokeStyle = '#ffffff';
    ctx.stroke();
    for (const h of [...vq, ...vq.map((p, i) => lerp(p, vq[(i + 1) % 4], 0.5))]) {
      ctx.fillStyle = '#ffffff';
      ctx.fillRect(Math.round(h.x) - 4, Math.round(h.y) - 4, 8, 8);
      ctx.strokeStyle = 'rgba(0,0,0,0.7)';
      ctx.strokeRect(Math.round(h.x) - 3.5, Math.round(h.y) - 3.5, 7, 7);
    }
    if (this.line) {
      const [a, b] = this.line.map(v);
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.strokeStyle = '#ffd34d';
      ctx.lineWidth = 2;
      ctx.stroke();
    }
    ctx.restore();
  }
}

export const crop = new CropState();
