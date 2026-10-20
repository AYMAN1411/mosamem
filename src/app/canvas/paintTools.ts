// Pointer handling for painting: brush, eraser, clone stamp, healing and spot healing (strokes
// streamed to the engine), gradient (drag a line), paint bucket and eyedropper (clicks).
//
// Stroke smoothing is a leash: the brush only moves once the pointer is further than the leash
// length away, then follows along the pointer's direction ("pulled string"). On release the
// stroke catches up to the pointer. Samples go to the engine at most once per frame.
import { app, viewportToDoc, docToViewport, type BrushSettings, type ToolId } from '../state.svelte';
import { engine, findLayer, type StrokeSpec } from '../../engine/bridge';
import { display } from '../../engine/display';
import { message } from '../actions';
import { t } from '../i18n/index.svelte';
import { nativePressure } from './pressure';

type Pt = { x: number; y: number };

export const STROKE_TOOLS: ToolId[] = ['brush', 'eraser', 'clone', 'heal', 'spotHeal'];
export const PAINT_TOOLS: ToolId[] = [...STROKE_TOOLS, 'gradient', 'bucket', 'eyedropper'];

const hex = (c: string): [number, number, number] => [parseInt(c.slice(1, 3), 16), parseInt(c.slice(3, 5), 16), parseInt(c.slice(5, 7), 16)];
const toHex = (c: number[]) => '#' + c.slice(0, 3).map((v) => v.toString(16).padStart(2, '0')).join('');

export function settingsFor(tool: ToolId): BrushSettings | null {
  const p = app.paint;
  switch (tool) {
    case 'brush':
      return p.brush;
    case 'eraser':
      return p.eraser;
    case 'clone':
      return p.clone;
    case 'heal':
    case 'spotHeal':
      return p.heal;
    default:
      return null;
  }
}

function fail(e: unknown) {
  app.notify(t('err.generic', { msg: message(e) }), 'error');
}

export class PaintInteraction {
  private stroke: { id: number; live: number | null; layer: number } | null = null;
  private starting: Promise<void> | null = null;
  private queue: [number, number, number][] = [];
  private inflight = false;
  private brushPos: Pt | null = null; // doc, after smoothing
  private pointer: Pt | null = null; // doc, raw
  private lastPressure = 1;
  /** Clone/heal source point (doc) and the persistent offset for aligned cloning. */
  private source: Pt | null = null;
  private offset: Pt | null = null;
  private gradient: { a: Pt; b: Pt } | null = null;
  hover: Pt | null = null; // viewport
  /** Test/perf hook: when an array, receives event-to-screen latency (ms) per batch. */
  latencies: number[] | null = null;
  /** Test hook: per-batch timing (event, sent, engine done, on screen). */
  trace: { start: number; sent: number; resp: number; drawn: number; n: number }[] | null = null;
  private batchStart = 0;

  constructor(private redraw: () => void) {}

  private toDoc(p: Pt): Pt {
    return viewportToDoc(app.active!.view, app.viewport, p.x, p.y);
  }

  get busy() {
    return !!this.stroke || !!this.starting || !!this.gradient;
  }

  private pressureOf(e: PointerEvent): number {
    if (e.pointerType === 'pen') return e.pressure || 0.0001;
    const native = nativePressure();
    if (native != null) return native; // macOS tablet events forwarded by the app
    return 1;
  }

  async down(e: PointerEvent, p: Pt) {
    const d = app.active;
    if (!d) return;
    const tool = app.tool;
    const doc = this.toDoc(p);
    // Alt: eyedropper with the brush/eraser/bucket/gradient, source point with clone/heal
    if (e.altKey && (tool === 'clone' || tool === 'heal')) {
      this.source = doc;
      this.offset = null;
      this.redraw();
      return;
    }
    if (tool === 'eyedropper' || (e.altKey && (tool === 'brush' || tool === 'bucket' || tool === 'gradient'))) {
      return this.pick(doc, e.altKey && tool === 'eyedropper');
    }
    const layer = findLayer(d.info.layers, d.info.active);
    if (!layer) return;
    if (tool === 'bucket') {
      const o = app.paint.bucket;
      try {
        app.update(await engine.paintBucket(d.info.id, { layer: layer.id, x: doc.x, y: doc.y, tolerance: o.tolerance, contiguous: o.contiguous, allLayers: o.allLayers, antialias: o.antialias, color: hex(app.fg), opacity: o.opacity }));
      } catch (err) {
        fail(err);
      }
      return;
    }
    if (tool === 'gradient') {
      this.gradient = { a: doc, b: doc };
      this.redraw();
      return;
    }
    if (!STROKE_TOOLS.includes(tool)) return;
    const s = settingsFor(tool)!;
    let offset: [number, number] | null = null;
    if (tool === 'clone' || tool === 'heal') {
      if (!this.source) {
        app.notify(t('paint.needSource'), 'info');
        return;
      }
      const aligned = tool === 'clone' ? app.paint.clone.aligned : true;
      if (!this.offset || !aligned) this.offset = { x: this.source.x - doc.x, y: this.source.y - doc.y };
      offset = [this.offset.x, this.offset.y];
    }
    const maskTarget = app.maskTarget && !!layer.mask;
    const spec: StrokeSpec = {
      tool: tool as StrokeSpec['tool'],
      target: maskTarget ? 'mask' : 'pixels',
      layer: layer.id,
      color: hex(tool === 'eraser' ? app.bg : app.fg),
      brush: { diameter: s.diameter, hardness: s.hardness, spacing: s.spacing, flow: s.flow, opacity: s.opacity, pressureSize: s.pressureSize, pressureOpacity: s.pressureOpacity, minSize: s.minSize },
      offset,
      sampleAll: tool === 'clone' ? app.paint.clone.sampleAll : tool === 'heal' || tool === 'spotHeal' ? app.paint.heal.sampleAll : false,
    };
    this.brushPos = doc;
    this.pointer = doc;
    this.lastPressure = this.pressureOf(e);
    this.queue = [[doc.x, doc.y, this.lastPressure]];
    this.batchStart = e.timeStamp || performance.now();
    this.starting = (async () => {
      try {
        const r = await engine.paintBegin(d.info.id, spec);
        this.stroke = { id: r.stroke, live: r.live, layer: layer.id };
        if (r.live != null && display.compositor) {
          display.compositor.setLive({ session: r.live, blend: layer.blend, alpha: layer.opacity * layer.fill, visible: layer.visible, layerKey: maskTarget ? -1 : layer.thumb });
        }
        this.flush();
      } catch (err) {
        fail(err);
        this.queue = [];
      } finally {
        this.starting = null;
      }
    })();
  }

  move(e: PointerEvent, p: Pt) {
    this.hover = p;
    if (!app.active) return;
    if (this.gradient) {
      let b = this.toDoc(p);
      if (e.shiftKey) b = this.snap45(this.gradient.a, b);
      this.gradient.b = b;
      this.redraw();
      return;
    }
    if (!this.brushPos || (!this.stroke && !this.starting)) {
      this.redraw();
      return;
    }
    const events = typeof e.getCoalescedEvents === 'function' ? e.getCoalescedEvents() : [];
    const samples = events.length ? events : [e];
    const stage = (e.currentTarget as HTMLElement | null) ?? document.querySelector('.stage');
    const r = stage!.getBoundingClientRect();
    if (!this.queue.length) this.batchStart = e.timeStamp || performance.now();
    for (const ev of samples) {
      const q = this.toDoc({ x: ev.clientX - r.left, y: ev.clientY - r.top });
      this.pointer = q;
      this.lastPressure = this.pressureOf(ev as PointerEvent);
      this.follow(q);
    }
    this.flush();
    this.redraw();
  }

  /** Leash smoothing: move the brush toward the pointer only beyond the leash length. */
  private follow(q: Pt) {
    const s = settingsFor(app.tool);
    const zoom = app.active!.view.zoom;
    const leash = ((s?.smoothing ?? 0) * 80) / zoom; // up to 80 screen px
    const b = this.brushPos!;
    const dx = q.x - b.x;
    const dy = q.y - b.y;
    const dist = Math.hypot(dx, dy);
    if (dist <= leash) return;
    const k = (dist - leash) / dist;
    this.brushPos = { x: b.x + dx * k, y: b.y + dy * k };
    this.queue.push([this.brushPos.x, this.brushPos.y, this.lastPressure]);
  }

  /** Sends queued samples; one request in flight at a time, the rest batch up. */
  private flush() {
    if (!this.stroke || this.inflight || !this.queue.length) return;
    const batch = this.queue;
    this.queue = [];
    this.inflight = true;
    const s = this.stroke;
    const started = this.batchStart || performance.now();
    const sent = performance.now();
    engine
      .paintPoints(s.id, s.live, batch)
      .then((keys) => {
        const resp = performance.now();
        const c = display.compositor;
        if (!keys.length || !c) return;
        const drawn = c.invalidate(s.live != null ? 2 : 0, keys);
        if (this.latencies) void drawn.then((t) => this.latencies?.push(t - started));
        if (this.trace) void drawn.then((t) => this.trace?.push({ start: started, sent, resp, drawn: t, n: batch.length }));
      })
      .catch(fail)
      .finally(() => {
        this.inflight = false;
        this.flush();
      });
  }

  async up(e: PointerEvent, p: Pt) {
    const d = app.active;
    if (!d) return;
    if (this.gradient) {
      const g = this.gradient;
      this.gradient = null;
      this.redraw();
      if (Math.hypot(g.b.x - g.a.x, g.b.y - g.a.y) < 1) return;
      const o = app.paint.gradient;
      const fg = hex(app.fg);
      const bg = hex(app.bg);
      try {
        app.update(
          await engine.paintGradient(d.info.id, {
            layer: d.info.active,
            kind: o.kind,
            a: [g.a.x, g.a.y],
            b: [g.b.x, g.b.y],
            from: [...fg, 255],
            to: o.toTransparent ? [...fg, 0] : [...bg, 255],
            opacity: o.opacity,
            reverse: o.reverse,
            dither: o.dither,
          }),
        );
      } catch (err) {
        fail(err);
      }
      return;
    }
    if (!this.brushPos) return;
    // catch up to where the pointer was released
    const q = this.toDoc(p);
    if (!this.queue.length) this.batchStart = e.timeStamp || performance.now();
    this.queue.push([q.x, q.y, this.lastPressure]);
    this.brushPos = null;
    if (this.starting) await this.starting;
    const s = this.stroke;
    if (!s) return;
    while (this.inflight || this.queue.length) {
      this.flush();
      await new Promise((r) => setTimeout(r, 4));
    }
    this.stroke = null;
    try {
      const info = await engine.paintEnd(s.id);
      app.update(info);
      // keep the engine's session: the next stroke on this layer reuses it (and its GPU tiles)
      const after = findLayer(info.layers, s.layer);
      if (s.live != null) display.compositor?.endLive(() => {}, app.maskTarget ? -1 : after?.thumb);
    } catch (err) {
      fail(err);
    }
  }

  private async pick(doc: Pt, toBackground: boolean) {
    const d = app.active!;
    const o = app.paint.eyedropper;
    try {
      const c = await engine.sampleColor(d.info.id, doc.x, doc.y, o.size, o.allLayers);
      if (c && c[3] > 0) {
        if (toBackground) app.bg = toHex(c);
        else app.fg = toHex(c);
      }
    } catch (err) {
      fail(err);
    }
  }

  private snap45(a: Pt, b: Pt): Pt {
    const ang = Math.round(Math.atan2(b.y - a.y, b.x - a.x) / (Math.PI / 4)) * (Math.PI / 4);
    const len = Math.hypot(b.x - a.x, b.y - a.y);
    return { x: a.x + Math.cos(ang) * len, y: a.y + Math.sin(ang) * len };
  }

  /** Brush ring, clone source marker and the gradient line, in viewport coordinates. */
  draw(ctx: CanvasRenderingContext2D) {
    const d = app.active;
    if (!d) return;
    const v = d.view;
    const vp = app.viewport;
    const ring = (x: number, y: number, r: number) => {
      ctx.beginPath();
      ctx.arc(x, y, Math.max(1.5, r), 0, Math.PI * 2);
      ctx.strokeStyle = 'rgba(0,0,0,0.75)';
      ctx.lineWidth = 1;
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(x, y, Math.max(1.5, r) + 1, 0, Math.PI * 2);
      ctx.strokeStyle = 'rgba(255,255,255,0.85)';
      ctx.stroke();
    };
    const s = settingsFor(app.tool);
    if (s && this.hover) {
      const r = (s.diameter * v.zoom) / 2;
      ring(this.hover.x, this.hover.y, r);
      if (r < 6) {
        ctx.beginPath();
        ctx.moveTo(this.hover.x - 6, this.hover.y);
        ctx.lineTo(this.hover.x + 6, this.hover.y);
        ctx.moveTo(this.hover.x, this.hover.y - 6);
        ctx.lineTo(this.hover.x, this.hover.y + 6);
        ctx.strokeStyle = 'rgba(255,255,255,0.9)';
        ctx.stroke();
      }
      // clone/heal: where the pixels come from
      if ((app.tool === 'clone' || app.tool === 'heal') && this.source) {
        const docHover = this.toDoc(this.hover);
        const off = this.offset ?? (this.stroke ? null : { x: this.source.x - docHover.x, y: this.source.y - docHover.y });
        const src = off && this.offset ? docToViewport(v, vp, docHover.x + off.x, docHover.y + off.y) : docToViewport(v, vp, this.source.x, this.source.y);
        ctx.beginPath();
        ctx.moveTo(src.x - 7, src.y);
        ctx.lineTo(src.x + 7, src.y);
        ctx.moveTo(src.x, src.y - 7);
        ctx.lineTo(src.x, src.y + 7);
        ctx.strokeStyle = '#fff';
        ctx.lineWidth = 2;
        ctx.stroke();
        ctx.strokeStyle = '#000';
        ctx.lineWidth = 1;
        ctx.stroke();
      }
    }
    if (this.gradient) {
      const a = docToViewport(v, vp, this.gradient.a.x, this.gradient.a.y);
      const b = docToViewport(v, vp, this.gradient.b.x, this.gradient.b.y);
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.strokeStyle = '#fff';
      ctx.lineWidth = 3;
      ctx.stroke();
      ctx.strokeStyle = '#000';
      ctx.lineWidth = 1;
      ctx.stroke();
      for (const q of [a, b]) {
        ctx.beginPath();
        ctx.arc(q.x, q.y, 3.5, 0, Math.PI * 2);
        ctx.fillStyle = '#fff';
        ctx.fill();
        ctx.stroke();
      }
    }
  }
}
