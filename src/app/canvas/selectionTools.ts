// Pointer handling for the selection tools: rectangle/ellipse marquee, lasso, polygon lasso,
// magic wand, quick selection and object selection (a click or a box; local AI), plus dragging
// the selection outline.
//
// Modifiers at the start of a gesture pick how the new shape combines with the selection
// (Shift = add, Alt = subtract, Shift+Alt = intersect). During a marquee drag, Shift keeps the
// shape square/circular and Alt draws from the center.
import { app, viewportToDoc, docToViewport, type ToolId } from '../state.svelte';
import { engine, type Combine, type DocInfo } from '../../engine/bridge';
import { display } from '../../engine/display';
import { message } from '../actions';
import { t } from '../i18n/index.svelte';
import { aiActions } from '../ai.svelte';

type Pt = { x: number; y: number };

export const SELECTION_TOOLS: ToolId[] = ['marqueeRect', 'marqueeEllipse', 'lasso', 'lassoPolygon', 'magicWand', 'quickSelect', 'objectSelect'];

function combineFrom(e: { shiftKey: boolean; altKey: boolean }): Combine {
  if (e.shiftKey && e.altKey) return 'intersect';
  if (e.shiftKey) return 'add';
  if (e.altKey) return 'subtract';
  return app.sel.mode;
}

async function apply(f: (doc: number) => Promise<DocInfo>) {
  const d = app.active;
  if (!d) return;
  try {
    app.update(await f(d.info.id));
  } catch (e) {
    app.notify(t('err.generic', { msg: message(e) }), 'error');
  }
}

export class SelectionInteraction {
  private start: Pt | null = null; // doc
  private cur: Pt | null = null; // doc
  private startView: Pt | null = null;
  private mode: Combine = 'replace';
  private center = false;
  private square = false;
  private lasso: Pt[] = [];
  private poly: Pt[] = [];
  private quick: Pt[] = [];
  private moving: { from: Pt } | null = null;
  hover: Pt | null = null; // viewport, for the quick-select brush ring

  constructor(private redraw: () => void) {}

  private toDoc(p: Pt): Pt {
    return viewportToDoc(app.active!.view, app.viewport, p.x, p.y);
  }

  get busy() {
    return !!(this.start || this.lasso.length || this.poly.length || this.quick.length || this.moving);
  }

  async down(e: PointerEvent, p: Pt) {
    const d = app.active;
    if (!d) return;
    const tool = app.tool;
    const doc = this.toDoc(p);
    this.mode = combineFrom(e);
    if (tool === 'lassoPolygon') {
      if (!this.poly.length) this.poly = [doc];
      else {
        const first = docToViewport(d.view, app.viewport, this.poly[0].x, this.poly[0].y);
        if (Math.hypot(first.x - p.x, first.y - p.y) < 7 || e.detail >= 2) return this.finishPolygon();
        this.poly.push(this.snap(doc, e));
      }
      this.cur = doc;
      this.redraw();
      return;
    }
    if (tool === 'magicWand') {
      const s = app.sel;
      await apply((id) => engine.selectWand(id, { x: doc.x, y: doc.y, tolerance: s.tolerance, contiguous: s.contiguous, allLayers: s.allLayers, antialias: s.antialias, mode: this.mode }));
      return;
    }
    if (tool === 'quickSelect') {
      this.quick = [doc];
      this.redraw();
      return;
    }
    if (tool === 'objectSelect') {
      // a click selects the object under it; a drag draws a box around one
      this.start = doc;
      this.cur = doc;
      this.startView = p;
      this.redraw();
      return;
    }
    // marquee / lasso: dragging inside an existing selection moves its outline
    if (this.mode === 'replace' && !e.shiftKey && !e.altKey && d.info.hasSelection && (await engine.selectHit(d.info.id, doc.x, doc.y))) {
      this.moving = { from: doc };
      return;
    }
    if (tool === 'lasso') {
      this.lasso = [doc];
    } else {
      this.start = doc;
      this.cur = doc;
      this.startView = p;
    }
    this.redraw();
  }

  move(e: PointerEvent, p: Pt) {
    if (!app.active) return;
    this.hover = p;
    const doc = this.toDoc(p);
    if (this.moving) {
      const c = display.compositor;
      if (c) {
        c.selectionOffset = [Math.round(doc.x - this.moving.from.x), Math.round(doc.y - this.moving.from.y)];
        c.requestFrame();
      }
      return;
    }
    if (this.lasso.length) {
      const last = this.lasso[this.lasso.length - 1];
      if (Math.hypot(last.x - doc.x, last.y - doc.y) * app.active.view.zoom > 1.5) this.lasso.push(doc);
    } else if (this.quick.length) {
      this.quick.push(doc);
    } else if (this.start && app.tool === 'objectSelect') {
      this.cur = doc;
    } else if (this.start) {
      this.square = e.shiftKey;
      this.center = e.altKey && this.mode !== 'subtract';
      this.cur = doc;
    } else if (this.poly.length) {
      this.cur = this.snap(doc, e);
    }
    this.redraw();
  }

  async up(e: PointerEvent, p: Pt) {
    const d = app.active;
    if (!d) return;
    if (this.moving) {
      const doc = this.toDoc(p);
      const dx = Math.round(doc.x - this.moving.from.x);
      const dy = Math.round(doc.y - this.moving.from.y);
      this.moving = null;
      await apply((id) => engine.selectTranslate(id, dx, dy));
      if (display.compositor) display.compositor.selectionOffset = [0, 0];
      return;
    }
    if (this.lasso.length) {
      const pts = this.lasso.map((q) => [q.x, q.y] as [number, number]);
      this.lasso = [];
      this.redraw();
      if (pts.length > 2) await apply((id) => engine.selectShape(id, { kind: 'polygon', points: pts, antialias: app.sel.antialias }, this.mode, app.sel.feather));
      return;
    }
    if (this.quick.length) {
      const pts = this.quick.map((q) => [q.x, q.y] as [number, number]);
      this.quick = [];
      this.redraw();
      const subtract = this.mode === 'subtract';
      await apply((id) => engine.selectQuick(id, pts, app.sel.brush / 2, app.sel.allLayers, subtract));
      return;
    }
    if (this.start && this.startView && app.tool === 'objectSelect') {
      const moved = Math.hypot(p.x - this.startView.x, p.y - this.startView.y) > 4;
      const s = this.start;
      const c = this.cur ?? s;
      this.start = null;
      this.startView = null;
      this.redraw();
      if (moved) await aiActions.objectSelect([], [Math.min(s.x, c.x), Math.min(s.y, c.y), Math.max(s.x, c.x), Math.max(s.y, c.y)], this.mode);
      else await aiActions.objectSelect([{ x: s.x, y: s.y, add: true }], null, this.mode);
      return;
    }
    if (this.start && this.startView) {
      const moved = Math.hypot(p.x - this.startView.x, p.y - this.startView.y) > 2;
      const box = this.box();
      this.start = null;
      this.startView = null;
      this.redraw();
      if (!moved && app.sel.style !== 'size') {
        // a click without a drag deselects
        if (this.mode === 'replace' && d.info.hasSelection) await apply((id) => engine.selectSimple(id, 'deselect'));
        return;
      }
      const shape = app.tool === 'marqueeEllipse' ? { kind: 'ellipse' as const, ...box, antialias: app.sel.antialias } : { kind: 'rect' as const, ...box };
      await apply((id) => engine.selectShape(id, shape, this.mode, app.sel.feather));
    }
  }

  /** Polygon lasso: Enter closes, Escape cancels, Backspace removes the last point. */
  key(e: KeyboardEvent): boolean {
    if (!this.poly.length) return false;
    if (e.key === 'Enter') {
      void this.finishPolygon();
      return true;
    }
    if (e.key === 'Escape') {
      this.poly = [];
      this.redraw();
      return true;
    }
    if (e.key === 'Backspace' || e.key === 'Delete') {
      this.poly.pop();
      this.redraw();
      return true;
    }
    return false;
  }

  cancel() {
    this.start = null;
    this.lasso = [];
    this.poly = [];
    this.quick = [];
    this.moving = null;
    if (display.compositor) display.compositor.selectionOffset = [0, 0];
    this.redraw();
  }

  private async finishPolygon() {
    const pts = this.poly.map((q) => [q.x, q.y] as [number, number]);
    this.poly = [];
    this.redraw();
    if (pts.length > 2) await apply((id) => engine.selectShape(id, { kind: 'polygon', points: pts, antialias: app.sel.antialias }, this.mode, app.sel.feather));
  }

  /** Shift snaps polygon edges to 45° steps (measured on screen). */
  private snap(doc: Pt, e: { shiftKey: boolean }): Pt {
    if (!e.shiftKey || !this.poly.length) return doc;
    const v = app.active!.view;
    const a = docToViewport(v, app.viewport, this.poly[this.poly.length - 1].x, this.poly[this.poly.length - 1].y);
    const b = docToViewport(v, app.viewport, doc.x, doc.y);
    const ang = Math.round(Math.atan2(b.y - a.y, b.x - a.x) / (Math.PI / 4)) * (Math.PI / 4);
    const len = Math.hypot(b.x - a.x, b.y - a.y);
    return viewportToDoc(v, app.viewport, a.x + Math.cos(ang) * len, a.y + Math.sin(ang) * len);
  }

  /** Marquee box in document coordinates, applying style, Shift (square) and Alt (from center). */
  private box() {
    const s = this.start!;
    const c = this.cur!;
    if (app.tool === 'objectSelect') return { x0: s.x, y0: s.y, x1: c.x, y1: c.y };
    let dx = c.x - s.x;
    let dy = c.y - s.y;
    const opts = app.sel;
    if (opts.style === 'ratio') {
      const r = opts.ratioW / Math.max(opts.ratioH, 1e-6);
      if (Math.abs(dx) / r > Math.abs(dy)) dy = Math.sign(dy || 1) * Math.abs(dx) / r;
      else dx = Math.sign(dx || 1) * Math.abs(dy) * r;
    } else if (opts.style === 'size') {
      dx = Math.sign(dx || 1) * opts.sizeW;
      dy = Math.sign(dy || 1) * opts.sizeH;
    } else if (this.square) {
      const m = Math.max(Math.abs(dx), Math.abs(dy));
      dx = Math.sign(dx || 1) * m;
      dy = Math.sign(dy || 1) * m;
    }
    const round = (v: number) => Math.round(v);
    if (this.center) return { x0: round(s.x - dx), y0: round(s.y - dy), x1: round(s.x + dx), y1: round(s.y + dy) };
    return { x0: round(s.x), y0: round(s.y), x1: round(s.x + dx), y1: round(s.y + dy) };
  }

  /** Draws the shape in progress (and the quick-select brush ring) in viewport coordinates. */
  draw(ctx: CanvasRenderingContext2D) {
    const d = app.active;
    if (!d) return;
    const v = d.view;
    const vp = app.viewport;
    const toV = (q: Pt) => docToViewport(v, vp, q.x, q.y);
    const path = (pts: Pt[], close: boolean) => {
      ctx.beginPath();
      pts.forEach((q, i) => {
        const s = toV(q);
        if (i) ctx.lineTo(s.x, s.y);
        else ctx.moveTo(s.x, s.y);
      });
      if (close) ctx.closePath();
    };
    const ants = () => {
      ctx.lineWidth = 1;
      ctx.setLineDash([4, 4]);
      ctx.strokeStyle = '#000';
      ctx.stroke();
      ctx.lineDashOffset = 4;
      ctx.strokeStyle = '#fff';
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.lineDashOffset = 0;
    };
    if (this.start && this.cur) {
      const b = this.box();
      if (app.tool === 'marqueeEllipse') {
        const n = 72;
        const cx = (b.x0 + b.x1) / 2;
        const cy = (b.y0 + b.y1) / 2;
        const rx = Math.abs(b.x1 - b.x0) / 2;
        const ry = Math.abs(b.y1 - b.y0) / 2;
        path(Array.from({ length: n }, (_, i) => ({ x: cx + rx * Math.cos((i / n) * Math.PI * 2), y: cy + ry * Math.sin((i / n) * Math.PI * 2) })), true);
      } else {
        path([{ x: b.x0, y: b.y0 }, { x: b.x1, y: b.y0 }, { x: b.x1, y: b.y1 }, { x: b.x0, y: b.y1 }], true);
      }
      ants();
    }
    if (this.lasso.length > 1) {
      path(this.lasso, false);
      ants();
    }
    if (this.poly.length) {
      path(this.cur ? [...this.poly, this.cur] : this.poly, false);
      ants();
      const f = toV(this.poly[0]);
      ctx.beginPath();
      ctx.arc(f.x, f.y, 4, 0, Math.PI * 2);
      ctx.strokeStyle = '#fff';
      ctx.stroke();
    }
    if (this.quick.length > 1) {
      path(this.quick, false);
      ctx.lineWidth = Math.max(1, app.sel.brush * v.zoom);
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
      ctx.strokeStyle = 'rgba(63,196,170,0.35)';
      ctx.stroke();
      ctx.lineWidth = 1;
    }
    if (app.tool === 'quickSelect' && this.hover) {
      ctx.beginPath();
      ctx.arc(this.hover.x, this.hover.y, Math.max(2, (app.sel.brush * v.zoom) / 2), 0, Math.PI * 2);
      ctx.strokeStyle = 'rgba(0,0,0,0.7)';
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(this.hover.x, this.hover.y, Math.max(2, (app.sel.brush * v.zoom) / 2) + 1, 0, Math.PI * 2);
      ctx.strokeStyle = 'rgba(255,255,255,0.8)';
      ctx.stroke();
    }
  }
}
