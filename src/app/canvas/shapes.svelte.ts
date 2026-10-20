// The shape tools: rectangle, ellipse, polygon, star and line. Drag to draw a new shape layer
// (Shift keeps it square, the line at 45° and the polygon's turn in 15° steps; Alt draws from the
// center). With a shape layer active, the options bar shows its fill and stroke and changes them.
import { engine, findLayer, walkLayers, type LayerInfo } from '../../engine/bridge';
import { dragShape, shapePath, tracePath, type Pt, type Shape, type ShapeData, type ShapeKind, type ShapePaint, type Stroke, type StrokeAlign } from '../../engine/vector';
import { app, docToViewport, viewportToDoc, type ToolId } from '../state.svelte';
import { t, type Key } from '../i18n/index.svelte';
import { message } from '../actions';

export const SHAPE_TOOLS: ToolId[] = ['shapeRect', 'shapeEllipse', 'shapePolygon', 'shapeStar', 'shapeLine'];
type DrawKind = Exclude<ShapeKind, 'free'>;
export const KIND_OF: Partial<Record<ToolId, DrawKind>> = { shapeRect: 'rect', shapeEllipse: 'ellipse', shapePolygon: 'polygon', shapeStar: 'star', shapeLine: 'line' };

export type Dash = 'solid' | 'dash' | 'dot';
export interface ShapeOptions {
  fillOn: boolean;
  fill: string;
  strokeOn: boolean;
  stroke: string;
  /** Stroke width; for a line, its weight. */
  width: number;
  align: StrokeAlign;
  dash: Dash;
  radius: number;
  sides: number;
  points: number;
  /** Inner radius of a star, as a fraction of the outer one. */
  inner: number;
  arrowStart: boolean;
  arrowEnd: boolean;
}

const DASHES: Record<Dash, number[]> = { solid: [], dash: [3, 2], dot: [0.001, 2] };
const dashOf = (d: number[]): Dash => (!d.length ? 'solid' : d[0] < 0.01 ? 'dot' : 'dash');
const rgba = (h: string): [number, number, number, number] => [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16), 255];
const hex = (c: number[]) => '#' + c.slice(0, 3).map((v) => v.toString(16).padStart(2, '0')).join('');

function strokeOf(o: ShapeOptions, line: boolean): Stroke {
  return { color: rgba(o.stroke), width: o.width, align: line ? 'center' : o.align, cap: o.dash === 'dot' ? 'round' : 'butt', join: 'miter', dash: DASHES[o.dash] };
}

/** Fill and stroke for a new shape from the options. A line is all stroke. */
export function paintOf(o: ShapeOptions, kind: ShapeKind): ShapePaint {
  if (kind === 'line') return { fill: null, stroke: strokeOf(o, true) };
  return { fill: o.fillOn ? { kind: 'solid', color: rgba(o.fill) } : null, stroke: o.strokeOn && o.width > 0 ? strokeOf(o, false) : null };
}

/** The options as a shape layer has them (what the options bar shows for it). */
export function optionsOf(d: ShapeData, base: ShapeOptions): ShapeOptions {
  const o = { ...base };
  const f = d.paint.fill;
  const s = d.paint.stroke;
  if (d.shape.kind !== 'line') {
    o.fillOn = !!f;
    o.strokeOn = !!s;
  }
  if (f?.kind === 'solid') o.fill = hex(f.color);
  if (s) Object.assign(o, { stroke: hex(s.color), width: s.width, align: s.align, dash: dashOf(s.dash) });
  const sh = d.shape;
  if (sh.kind === 'rect') o.radius = sh.radius;
  if (sh.kind === 'polygon') o.sides = sh.sides;
  if (sh.kind === 'star') Object.assign(o, { points: sh.points, inner: sh.inner });
  if (sh.kind === 'line') Object.assign(o, { arrowStart: sh.arrowStart, arrowEnd: sh.arrowEnd });
  return o;
}

/** A shape layer's data with the changed options applied (only what the patch touches). */
export function patched(d: ShapeData, o: ShapeOptions, patch: Partial<ShapeOptions>): ShapeData {
  const has = (...k: (keyof ShapeOptions)[]) => k.some((x) => x in patch);
  const line = d.shape.kind === 'line';
  const paint = { ...d.paint };
  if (has('fillOn', 'fill') && !line) paint.fill = o.fillOn ? { kind: 'solid', color: rgba(o.fill) } : null;
  if (has('strokeOn', 'stroke', 'width', 'align', 'dash')) paint.stroke = line || (o.strokeOn && o.width > 0) ? { ...strokeOf(o, line), join: d.paint.stroke?.join ?? 'miter' } : null;
  let shape: Shape = d.shape;
  if (shape.kind === 'rect' && has('radius')) shape = { ...shape, radius: Math.max(0, o.radius) };
  if (shape.kind === 'polygon' && has('sides')) shape = { ...shape, sides: Math.max(3, Math.round(o.sides)) };
  if (shape.kind === 'star' && has('points', 'inner')) shape = { ...shape, points: Math.max(3, Math.round(o.points)), inner: Math.min(1, Math.max(0.01, o.inner)) };
  if (shape.kind === 'line' && has('arrowStart', 'arrowEnd')) shape = { ...shape, arrowStart: o.arrowStart, arrowEnd: o.arrowEnd };
  return { shape, paint };
}

function fail(e: unknown) {
  app.notify(t('err.generic', { msg: message(e) }), 'error');
}

/** The active layer, if it is a shape layer. */
export function activeShape(): LayerInfo | null {
  const d = app.active;
  const l = d ? findLayer(d.info.layers, d.info.active) : null;
  return l?.kind === 'shape' && l.shape ? l : null;
}

class ShapeTool {
  opts = $state<ShapeOptions>({
    fillOn: true, fill: '#3f7fd8', strokeOn: false, stroke: '#000000', width: 4, align: 'inside', dash: 'solid',
    radius: 0, sides: 6, points: 5, inner: 0.5, arrowStart: false, arrowEnd: true,
  });
  private drag: { a: Pt; b: Pt; square: boolean; center: boolean; kind: DrawKind } | null = null;
  private redraw: () => void = () => {};

  setRedraw(f: () => void) {
    this.redraw = f;
  }

  /** What the options bar shows: the active shape layer's settings, or the tool's. */
  current(): ShapeOptions {
    const l = activeShape();
    return l ? optionsOf(l.shape!, this.opts) : this.opts;
  }

  /** Changes the options, and the active shape layer with them. */
  async set(patch: Partial<ShapeOptions>) {
    const o = { ...this.current(), ...patch };
    this.opts = o;
    const l = activeShape();
    if (l) await this.update(l, patched(l.shape!, o, patch), false);
  }

  /** New geometry or paint for a shape layer (`live` while a value is being dragged). */
  async update(l: LayerInfo, data: ShapeData, live: boolean) {
    const d = app.active;
    if (!d) return;
    try {
      app.update(await engine.shapeSet(d.info.id, l.id, data, live));
    } catch (e) {
      fail(e);
    }
  }

  private toDoc(p: { x: number; y: number }): Pt {
    const d = app.active!;
    const q = viewportToDoc(d.view, app.viewport, p.x, p.y);
    return [q.x, q.y];
  }

  private shape(): Shape | null {
    const dr = this.drag;
    if (!dr) return null;
    const o = this.opts;
    return dragShape(dr.kind, dr.a, dr.b, { square: dr.square, center: dr.center, radius: o.radius, sides: o.sides, inner: o.inner, points: o.points, arrowStart: o.arrowStart, arrowEnd: o.arrowEnd });
  }

  down(e: PointerEvent, p: { x: number; y: number }) {
    const kind = KIND_OF[app.tool];
    if (!app.active || !kind) return;
    // the options bar shows the selected shape layer's settings; a new shape takes what it shows
    const l = activeShape();
    if (l) this.opts = optionsOf(l.shape!, this.opts);
    const a = this.toDoc(p);
    this.drag = { a, b: a, square: e.shiftKey, center: e.altKey, kind };
    this.redraw();
  }

  move(e: PointerEvent, p: { x: number; y: number }) {
    if (!this.drag || !app.active) return;
    Object.assign(this.drag, { b: this.toDoc(p), square: e.shiftKey, center: e.altKey });
    this.redraw();
  }

  async up(e: PointerEvent, p: { x: number; y: number }) {
    const dr = this.drag;
    const d = app.active;
    if (!dr || !d) return;
    Object.assign(dr, { b: this.toDoc(p), square: e.shiftKey, center: e.altKey });
    const span = Math.hypot(dr.b[0] - dr.a[0], dr.b[1] - dr.a[1]) * d.view.zoom;
    if (span < 3) {
      // a click: a 100 px shape at the click (a line needs a drag)
      if (dr.kind === 'line') {
        this.drag = null;
        this.redraw();
        return;
      }
      const round = dr.kind === 'polygon' || dr.kind === 'star';
      dr.b = round ? [dr.a[0], dr.a[1] - 50] : [dr.a[0] + 100, dr.a[1] + 100];
    }
    const shape = this.shape()!;
    this.drag = null;
    this.redraw();
    try {
      app.update(await engine.shapeNew(d.info.id, { shape, paint: paintOf(this.opts, dr.kind) }, this.newName(dr.kind)));
    } catch (err) {
      fail(err);
    }
  }

  cancel() {
    this.drag = null;
    this.redraw();
  }

  /** "Rectangle 3": one more than the shape layers of that kind. */
  private newName(kind: DrawKind) {
    let n = 1;
    walkLayers(app.active!.info.layers, (l) => {
      if (l.kind === 'shape' && l.shape?.shape.kind === kind) n++;
    });
    return `${t(`shape.${kind}` as Key)} ${n}`;
  }

  /** The shape being dragged, painted roughly as it will look, with a thin outline. */
  draw(ctx: CanvasRenderingContext2D) {
    const d = app.active;
    const s = this.shape();
    if (!d || !s || !this.drag) return;
    const z = d.view.zoom;
    const to = (q: Pt): Pt => {
      const v = docToViewport(d.view, app.viewport, q[0], q[1]);
      return [v.x, v.y];
    };
    const paint = paintOf(this.opts, s.kind);
    ctx.save();
    tracePath(ctx, shapePath(s), to);
    if (paint.fill?.kind === 'solid') {
      ctx.fillStyle = hex(paint.fill.color);
      ctx.fill();
    }
    if (paint.stroke) {
      ctx.strokeStyle = hex(paint.stroke.color);
      ctx.lineWidth = Math.max(1, paint.stroke.width * z);
      ctx.lineCap = paint.stroke.cap;
      ctx.stroke();
    }
    ctx.strokeStyle = '#2f9bff';
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.restore();
  }
}

export const shapeTool = new ShapeTool();
