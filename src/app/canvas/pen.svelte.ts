// The Pen tool and the Direct Selection tool.
//
// Pen: click for a corner point, drag for a smooth point with handles; click the first point to
// close the path, Enter or Esc to finish it open. Finished, it becomes a shape layer (Shape mode)
// or the document's work path (Path mode). Without a path in progress, a click on the active
// path's outline adds a point and a click on a point deletes it.
//
// Direct Selection: click a point to select it (Shift adds), drag points, or drag a selected
// point's handle (Alt breaks a smooth point into a corner). A drag on a segment moves its two
// points. Delete removes the selected points. A rectangle or another live shape becomes a free
// path once its points are moved.
import { engine, findLayer, walkLayers, type LayerInfo } from '../../engine/bridge';
import { clonePath, insertKnot, nearestSegment, shapePath, segments, type Knot, type Path, type Pt, type ShapeData } from '../../engine/vector';
import { app, docToViewport, viewportToDoc, type ToolId } from '../state.svelte';
import { t } from '../i18n/index.svelte';
import { message } from '../actions';
import { activeShape, paintOf, shapeTool } from './shapes.svelte';

export const PATH_TOOLS: ToolId[] = ['pen', 'directSelect'];

type Target = { kind: 'layer'; id: number } | { kind: 'work' };
type KnotRef = [number, number];
type Drag =
  | { kind: 'pull'; k: number; closing: boolean }
  | { kind: 'knots'; from: Pt; base: Path; target: Target; moved: boolean }
  | { kind: 'handle'; k: KnotRef; which: 'i' | 'o'; base: Path; target: Target; moved: boolean };

const sub = (a: Pt, b: Pt): Pt => [a[0] - b[0], a[1] - b[1]];
const add = (a: Pt, b: Pt): Pt => [a[0] + b[0], a[1] + b[1]];
const corner = (p: Pt): Knot => ({ p, i: p, o: p, smooth: false });

function fail(e: unknown) {
  app.notify(t('err.generic', { msg: message(e) }), 'error');
}

/** The path the tools work on: the active shape layer's, or else the work path. */
function targetPath(): { target: Target; path: Path; layer?: LayerInfo } | null {
  const l = activeShape();
  if (l) return { target: { kind: 'layer', id: l.id }, path: shapePath(l.shape!.shape), layer: l };
  const wp = app.active?.workPath;
  return wp ? { target: { kind: 'work' }, path: wp } : null;
}
const targetKey = (t: Target) => (t.kind === 'work' ? 'work' : `layer:${t.id}`);

class PenTool {
  mode = $state<'shape' | 'path'>('shape');
  /** The path being drawn with the Pen, until it is finished. */
  drawing = $state<{ doc: number; path: Path } | null>(null);
  /** Points selected with Direct Selection, and the path they belong to. */
  selected = $state<{ key: string; knots: KnotRef[] }>({ key: '', knots: [] });
  /** A path being dragged, shown instead of its layer's until the engine has it. */
  private editing: { target: Target; path: Path } | null = null;
  private drag: Drag | null = null;
  private hover: Pt | null = null;
  private redraw: () => void = () => {};
  private inflight = false;
  private queued: { id: number; data: ShapeData } | null = null;

  setRedraw(f: () => void) {
    this.redraw = f;
  }

  private toDoc(p: { x: number; y: number }): Pt {
    const d = app.active!;
    const q = viewportToDoc(d.view, app.viewport, p.x, p.y);
    return [q.x, q.y];
  }
  /** Hit distance: 6 screen px. */
  private tol() {
    return 6 / (app.active?.view.zoom ?? 1);
  }

  private hitKnot(path: Path, at: Pt): KnotRef | null {
    const tol = this.tol();
    for (let s = path.subs.length - 1; s >= 0; s--) {
      const ks = path.subs[s].knots;
      for (let k = 0; k < ks.length; k++) if (Math.hypot(ks[k].p[0] - at[0], ks[k].p[1] - at[1]) <= tol) return [s, k];
    }
    return null;
  }

  private selectedIn(target: Target): KnotRef[] {
    return this.selected.key === targetKey(target) ? this.selected.knots : [];
  }

  /** Saves a changed path: to its shape layer (as a free path) or to the work path. */
  private async save(target: Target, path: Path, live: boolean) {
    const d = app.active;
    if (!d) return;
    if (target.kind === 'work') {
      d.workPath = path;
      this.redraw();
      return;
    }
    const l = findLayer(d.info.layers, target.id);
    if (!l?.shape) return;
    const data: ShapeData = { shape: { kind: 'free', path }, paint: l.shape.paint };
    if (!live) {
      while (this.inflight) await new Promise((r) => setTimeout(r, 5));
      this.queued = null;
      await shapeTool.update(l, data, false);
      return;
    }
    // at most one live update in flight; the latest wins
    this.queued = { id: l.id, data };
    if (this.inflight) return;
    this.inflight = true;
    while (this.queued) {
      const q = this.queued;
      this.queued = null;
      try {
        app.update(await engine.shapeSet(d.info.id, q.id, q.data, true));
      } catch (e) {
        fail(e);
      }
    }
    this.inflight = false;
  }

  // ---- Pen ----

  penDown(e: PointerEvent, p: { x: number; y: number }) {
    const d = app.active;
    if (!d) return;
    const at = this.toDoc(p);
    const dr = this.drawing;
    if (!dr || dr.doc !== d.info.id) {
      // on the active path: a point deletes it, the outline gets a new point
      const tp = targetPath();
      if (tp && !e.altKey) {
        const kn = this.hitKnot(tp.path, at);
        if (kn) {
          void this.deleteKnots(tp.target, tp.path, [kn]);
          return;
        }
        const seg = nearestSegment(tp.path, at, this.tol());
        if (seg) {
          const path = clonePath(tp.path);
          const k = insertKnot(path, seg.sub, seg.seg, seg.t);
          this.selected = { key: targetKey(tp.target), knots: [[seg.sub, k]] };
          void this.save(tp.target, path, false);
          return;
        }
      }
      this.drawing = { doc: d.info.id, path: { subs: [{ knots: [corner(at)], closed: false }] } };
      this.drag = { kind: 'pull', k: 0, closing: false };
    } else {
      const s = dr.path.subs[0];
      if (s.knots.length >= 2 && Math.hypot(s.knots[0].p[0] - at[0], s.knots[0].p[1] - at[1]) <= this.tol()) {
        s.closed = true;
        this.drag = { kind: 'pull', k: 0, closing: true };
      } else {
        s.knots.push(corner(at));
        this.drag = { kind: 'pull', k: s.knots.length - 1, closing: false };
      }
    }
    this.redraw();
  }

  penMove(_e: PointerEvent, p: { x: number; y: number }) {
    if (!app.active) return;
    const at = this.toDoc(p);
    this.hover = at;
    const dg = this.drag;
    if (dg?.kind === 'pull' && this.drawing) {
      const kn = this.drawing.path.subs[0].knots[dg.k];
      // a drag pulls out the handles of the new point, the same on both sides
      if (Math.hypot(at[0] - kn.p[0], at[1] - kn.p[1]) * app.active.view.zoom > 2) {
        kn.o = at;
        kn.i = sub(kn.p, sub(at, kn.p));
        kn.smooth = true;
      }
    }
    this.redraw();
  }

  async penUp() {
    const dg = this.drag;
    this.drag = null;
    if (dg?.kind === 'pull' && dg.closing) await this.finish();
    this.redraw();
  }

  /** Ends the path being drawn: a new shape layer, or the work path. */
  async finish() {
    const dr = this.drawing;
    this.drawing = null;
    this.drag = null;
    this.redraw();
    if (!dr) return;
    const path = clonePath($state.snapshot(dr.path) as Path);
    if (path.subs[0].knots.length < 2) return;
    const doc = app.docs.find((x) => x.info.id === dr.doc);
    if (!doc) return;
    if (this.mode === 'path') {
      doc.workPath = path;
      this.redraw();
      return;
    }
    let n = 1;
    walkLayers(doc.info.layers, (l) => {
      if (l.kind === 'shape' && l.shape?.shape.kind === 'free') n++;
    });
    try {
      app.update(await engine.shapeNew(dr.doc, { shape: { kind: 'free', path }, paint: paintOf(shapeTool.opts, 'free') }, `${t('shape.free')} ${n}`));
    } catch (e) {
      fail(e);
    }
  }

  // ---- Direct Selection ----

  selectDown(e: PointerEvent, p: { x: number; y: number }) {
    const tp = targetPath();
    if (!tp || !app.active) return;
    const at = this.toDoc(p);
    const path = tp.path;
    const key = targetKey(tp.target);
    const tol = this.tol();
    // a selected point's handles come first
    for (const [s, k] of this.selectedIn(tp.target)) {
      const kn = path.subs[s]?.knots[k];
      if (!kn) continue;
      for (const which of ['i', 'o'] as const) {
        const h = kn[which];
        if ((h[0] !== kn.p[0] || h[1] !== kn.p[1]) && Math.hypot(h[0] - at[0], h[1] - at[1]) <= tol) {
          this.drag = { kind: 'handle', k: [s, k], which, base: clonePath(path), target: tp.target, moved: false };
          return;
        }
      }
    }
    const kn = this.hitKnot(path, at);
    if (kn) {
      const sel = this.selectedIn(tp.target);
      const has = sel.some(([s, k]) => s === kn[0] && k === kn[1]);
      if (e.shiftKey) this.selected = { key, knots: has ? sel.filter(([s, k]) => s !== kn[0] || k !== kn[1]) : [...sel, kn] };
      else if (!has) this.selected = { key, knots: [kn] };
    } else {
      const seg = nearestSegment(path, at, tol);
      if (!seg) {
        this.selected = { key, knots: [] };
        this.redraw();
        return;
      }
      const n = path.subs[seg.sub].knots.length;
      this.selected = { key, knots: [[seg.sub, seg.seg], [seg.sub, (seg.seg + 1) % n]] };
    }
    this.drag = { kind: 'knots', from: at, base: clonePath(path), target: tp.target, moved: false };
    this.redraw();
  }

  selectMove(e: PointerEvent, p: { x: number; y: number }) {
    const dg = this.drag;
    if (!dg || dg.kind === 'pull' || !app.active) return;
    const at = this.toDoc(p);
    const path = clonePath(dg.base);
    if (dg.kind === 'knots') {
      const delta = sub(at, dg.from);
      if (!dg.moved && Math.hypot(delta[0], delta[1]) * app.active.view.zoom < 2) return;
      dg.moved = true;
      for (const [s, k] of this.selected.knots) {
        const kn = path.subs[s]?.knots[k];
        if (kn) Object.assign(kn, { p: add(kn.p, delta), i: add(kn.i, delta), o: add(kn.o, delta) });
      }
    } else {
      dg.moved = true;
      const kn = path.subs[dg.k[0]].knots[dg.k[1]];
      kn[dg.which] = at;
      if (e.altKey) kn.smooth = false;
      else if (kn.smooth) {
        // a smooth point keeps its handles in line, each with its own length
        const other = dg.which === 'i' ? 'o' : 'i';
        const v = sub(at, kn.p);
        const len = Math.hypot(v[0], v[1]);
        const olen = Math.hypot(kn[other][0] - kn.p[0], kn[other][1] - kn.p[1]) || len;
        if (len > 1e-9) kn[other] = [kn.p[0] - (v[0] / len) * olen, kn.p[1] - (v[1] / len) * olen];
      }
    }
    this.editing = { target: dg.target, path };
    void this.save(dg.target, path, true);
    this.redraw();
  }

  async selectUp() {
    const dg = this.drag;
    this.drag = null;
    const ed = this.editing;
    this.editing = null;
    if (dg && dg.kind !== 'pull' && dg.moved && ed) await this.save(ed.target, ed.path, false);
    this.redraw();
  }

  /** Delete / Backspace with Direct Selection: removes the selected points. */
  async deleteSelected(): Promise<boolean> {
    const tp = targetPath();
    const sel = tp ? this.selectedIn(tp.target) : [];
    if (!tp || !sel.length) return false;
    await this.deleteKnots(tp.target, tp.path, sel);
    return true;
  }

  private async deleteKnots(target: Target, base: Path, knots: KnotRef[]) {
    const path = clonePath(base);
    const gone = new Set(knots.map(([s, k]) => `${s}:${k}`));
    path.subs = path.subs.map((s, si) => ({ ...s, knots: s.knots.filter((_, k) => !gone.has(`${si}:${k}`)) })).filter((s) => s.knots.length >= 2);
    this.selected = { key: targetKey(target), knots: [] };
    const d = app.active;
    if (!d) return;
    if (path.subs.length) await this.save(target, path, false);
    else if (target.kind === 'work') d.workPath = null;
    else {
      try {
        app.update(await engine.layerDelete(d.info.id, [target.id]));
      } catch (e) {
        fail(e);
      }
    }
    this.redraw();
  }

  /** Keys: Enter and Esc finish the Pen's path; Delete removes selected points. */
  key(e: KeyboardEvent): boolean {
    if (app.tool === 'pen' && this.drawing && (e.key === 'Enter' || e.key === 'Escape')) {
      void this.finish();
      return true;
    }
    if (app.tool === 'directSelect' && (e.key === 'Delete' || e.key === 'Backspace') && this.selected.knots.length) {
      void this.deleteSelected();
      return true;
    }
    return false;
  }

  cancel() {
    this.drag = null;
    this.editing = null;
    this.redraw();
  }

  // ---- drawing on the overlay ----

  draw(ctx: CanvasRenderingContext2D) {
    const d = app.active;
    if (!d || !PATH_TOOLS.includes(app.tool)) return;
    const to = (q: Pt): Pt => {
      const v = docToViewport(d.view, app.viewport, q[0], q[1]);
      return [v.x, v.y];
    };
    const blue = '#2f9bff';
    const outline = (path: Path) => {
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
      ctx.strokeStyle = blue;
      ctx.lineWidth = 1;
      ctx.stroke();
    };
    const handles = (kn: Knot) => {
      for (const h of [kn.i, kn.o]) {
        if (h[0] === kn.p[0] && h[1] === kn.p[1]) continue;
        const [a, b] = [to(kn.p), to(h)];
        ctx.beginPath();
        ctx.moveTo(a[0], a[1]);
        ctx.lineTo(b[0], b[1]);
        ctx.strokeStyle = blue;
        ctx.stroke();
        ctx.beginPath();
        ctx.arc(b[0], b[1], 3, 0, Math.PI * 2);
        ctx.fillStyle = blue;
        ctx.fill();
      }
    };
    const knot = (kn: Knot, on: boolean) => {
      const [x, y] = to(kn.p);
      ctx.fillStyle = on ? blue : '#ffffff';
      ctx.strokeStyle = blue;
      ctx.fillRect(x - 3.5, y - 3.5, 7, 7);
      ctx.strokeRect(x - 3.5, y - 3.5, 7, 7);
    };
    ctx.save();
    const dr = this.drawing;
    if (dr && dr.doc === d.info.id) {
      const s = dr.path.subs[0];
      outline(dr.path);
      // where the next click would go
      const last = s.knots[s.knots.length - 1];
      if (this.hover && !this.drag && !s.closed) {
        const [a, c1, b] = [to(last.p), to(last.o), to(this.hover)];
        ctx.beginPath();
        ctx.moveTo(a[0], a[1]);
        ctx.quadraticCurveTo(c1[0], c1[1], b[0], b[1]);
        ctx.setLineDash([4, 3]);
        ctx.strokeStyle = blue;
        ctx.stroke();
        ctx.setLineDash([]);
      }
      handles(last);
      s.knots.forEach((kn, k) => knot(kn, k === s.knots.length - 1));
    } else {
      const tp = targetPath();
      if (tp) {
        const path = this.editing && targetKey(this.editing.target) === targetKey(tp.target) ? this.editing.path : tp.path;
        outline(path);
        const sel = this.selectedIn(tp.target);
        if (app.tool === 'directSelect') for (const [s, k] of sel) if (path.subs[s]?.knots[k]) handles(path.subs[s].knots[k]);
        path.subs.forEach((s, si) => s.knots.forEach((kn, k) => knot(kn, sel.some(([a, b]) => a === si && b === k))));
      }
    }
    ctx.restore();
  }
}

export const pen = new PenTool();
