// The Text tool. Click for point text, drag for a text box, click a text layer to edit it.
//
// While typing, a plain-text editor sits over the canvas in the same bundled font, size and
// direction, mapped through the layer's placement and the view (so caret movement, selection,
// input methods and mixed Arabic/English behave natively). The engine draws the layer when the
// edit is committed; the layer is hidden meanwhile so the text isn't shown twice.
import { engine, walkLayers, type LayerInfo } from '../../engine/bridge';
import { defaultTextStyle, type TextData, type TextStyle } from '../../engine/text';
import type { Pt } from '../../engine/matrix';
import { app, docToViewport, viewportToDoc } from '../state.svelte';
import { t } from '../i18n/index.svelte';
import { message } from '../actions';

interface Editing {
  doc: number;
  /** The layer being edited, or null for new text. */
  id: number | null;
  data: TextData;
  wasVisible: boolean;
}

class TextTool {
  editing = $state<Editing | null>(null);
  /** Style for new text (and what the options bar shows when not editing). */
  style = $state<TextStyle>(defaultTextStyle());
  /** [ascent, descent] of the edited style, for placing the editor on the baseline. */
  metrics = $state<[number, number]>([58, 14]);
  private drag: { a: Pt; b: Pt } | null = null;
  /** The hit test of the last press (whether it landed on a text layer), until it's answered. */
  private pressing: Promise<void> | null = null;
  private redraw: () => void = () => {};
  private busy = false;

  setRedraw(f: () => void) {
    this.redraw = f;
  }

  private async loadMetrics(style: TextStyle) {
    this.metrics = await engine.textMetrics(style).catch(() => [style.size * 0.8, style.size * 0.2] as [number, number]);
  }

  /** The style shown in the options bar: the one being typed, the active text layer's, or the tool's. */
  currentStyle(): TextStyle {
    if (this.editing) return this.editing.data.style;
    const d = app.active;
    const l = d ? findText(d.info.layers, d.info.active) : null;
    return l?.text?.style ?? this.style;
  }

  /** Applies a style change: to the text being typed, or to the active text layer, and to new text. */
  async setStyle(patch: Partial<TextStyle>) {
    this.style = { ...this.style, ...patch };
    if (this.editing) {
      this.editing.data = { ...this.editing.data, style: { ...this.editing.data.style, ...patch } };
      void this.loadMetrics(this.editing.data.style);
      return;
    }
    const d = app.active;
    const l = d ? findText(d.info.layers, d.info.active) : null;
    if (d && l?.text) {
      try {
        app.update(await engine.textSet(d.info.id, l.id, { ...l.text, style: { ...l.text.style, ...patch } }, false));
      } catch (e) {
        app.notify(t('err.generic', { msg: message(e) }), 'error');
      }
    }
  }

  setText(text: string) {
    if (this.editing) this.editing.data = { ...this.editing.data, text };
  }

  private startNew(data: TextData) {
    const d = app.active;
    if (!d) return;
    this.editing = { doc: d.info.id, id: null, data, wasVisible: true };
    void this.loadMetrics(data.style);
    this.redraw();
  }

  async edit(layer: LayerInfo) {
    const d = app.active;
    if (!d || !layer.text) return;
    if (this.editing) await this.commit();
    this.editing = { doc: d.info.id, id: layer.id, data: JSON.parse(JSON.stringify(layer.text)), wasVisible: layer.visible };
    void this.loadMetrics(layer.text.style);
    if (layer.visible) app.update(await engine.layerSet(d.info.id, layer.id, { visible: false }, true));
    this.redraw();
  }

  async commit() {
    const ed = this.editing;
    if (!ed || this.busy) return;
    this.busy = true;
    this.editing = null;
    try {
      const empty = !ed.data.text.trim();
      if (ed.id == null) {
        if (!empty) app.update(await engine.textNew(ed.doc, ed.data));
      } else {
        if (ed.wasVisible) app.update(await engine.layerSet(ed.doc, ed.id, { visible: true }, true));
        // an emptied text layer goes away, as in other editors
        app.update(empty ? await engine.layerDelete(ed.doc, [ed.id]) : await engine.textSet(ed.doc, ed.id, ed.data, false));
      }
    } catch (e) {
      app.notify(t('err.generic', { msg: message(e) }), 'error');
    } finally {
      this.busy = false;
      this.redraw();
    }
  }

  async cancel() {
    const ed = this.editing;
    this.editing = null;
    if (ed?.id != null && ed.wasVisible) app.update(await engine.layerSet(ed.doc, ed.id, { visible: true }, true));
    this.redraw();
  }

  // ---- pointer (the Text tool on the canvas) ----

  async down(e: PointerEvent, p: Pt) {
    const d = app.active;
    if (!d) return;
    if (this.editing) {
      // a press outside the editor commits (the editor itself takes presses inside it)
      await this.commit();
      return;
    }
    const at = viewportToDoc(d.view, app.viewport, p.x, p.y);
    // the press starts a new text right away; the engine's answer to "is there a text layer
    // here?" comes later, and a quick click's release must not arrive to nothing
    this.drag = { a: at, b: at };
    this.pressing = this.hitText(at).then(async (hit) => {
      if (hit) {
        // on a text layer: edit it
        this.drag = null;
        await this.edit(hit);
      }
    });
    await this.pressing;
    this.pressing = null;
  }

  move(_e: PointerEvent, p: Pt) {
    const d = app.active;
    if (!d || !this.drag) return;
    this.drag.b = viewportToDoc(d.view, app.viewport, p.x, p.y);
    this.redraw();
  }

  async up() {
    if (this.pressing) await this.pressing;
    const dr = this.drag;
    const d = app.active;
    this.drag = null;
    if (!dr || !d) return;
    const style = { ...this.style };
    const w = Math.abs(dr.b.x - dr.a.x);
    const h = Math.abs(dr.b.y - dr.a.y);
    // a drag of a few screen pixels is a click: point text on the baseline at the click
    if (w * d.view.zoom < 6 && h * d.view.zoom < 6) {
      this.startNew({ text: '', style, width: null, height: null, matrix: [1, 0, Math.round(dr.a.x), 0, 1, Math.round(dr.a.y)] });
    } else {
      const x = Math.min(dr.a.x, dr.b.x);
      const y = Math.min(dr.a.y, dr.b.y);
      this.startNew({ text: '', style, width: Math.round(w), height: Math.round(h), matrix: [1, 0, Math.round(x), 0, 1, Math.round(y)] });
    }
  }

  private async hitText(at: Pt): Promise<LayerInfo | null> {
    const d = app.active;
    if (!d) return null;
    const texts: LayerInfo[] = [];
    walkLayers(d.info.layers, (l) => {
      if (l.kind === 'text' && l.visible) texts.push(l);
    });
    // top to bottom: the first one under the point wins
    for (const l of texts) {
      const b = await engine.transformBounds(d.info.id, [l.id]).catch(() => null);
      if (b && at.x >= b.x - 4 && at.x <= b.x + b.w + 4 && at.y >= b.y - 4 && at.y <= b.y + b.h + 4) return l;
    }
    return null;
  }

  draw(ctx: CanvasRenderingContext2D) {
    const d = app.active;
    if (!d) return;
    const v = (q: Pt) => docToViewport(d.view, app.viewport, q.x, q.y);
    const rect = (x: number, y: number, w: number, h: number) => {
      const c = [v({ x, y }), v({ x: x + w, y }), v({ x: x + w, y: y + h }), v({ x, y: y + h })];
      ctx.beginPath();
      c.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)));
      ctx.closePath();
      ctx.save();
      ctx.setLineDash([4, 3]);
      ctx.strokeStyle = 'rgba(0,0,0,0.6)';
      ctx.lineWidth = 3;
      ctx.stroke();
      ctx.strokeStyle = '#ffffff';
      ctx.lineWidth = 1;
      ctx.stroke();
      ctx.restore();
    };
    if (this.drag) {
      const { a, b } = this.drag;
      rect(Math.min(a.x, b.x), Math.min(a.y, b.y), Math.abs(b.x - a.x), Math.abs(b.y - a.y));
    }
    const ed = this.editing;
    if (ed && ed.doc === d.info.id && ed.data.width != null) {
      const m = ed.data.matrix;
      // the box frame (axis-aligned boxes; turned text shows only the editor)
      if (Math.abs(m[1]) < 1e-9 && Math.abs(m[3]) < 1e-9) rect(m[2], m[5], ed.data.width * m[0], (ed.data.height ?? ed.data.style.size * 1.2) * m[4]);
    }
  }
}

function findText(layers: LayerInfo[], id: number): LayerInfo | null {
  let out: LayerInfo | null = null;
  walkLayers(layers, (l) => {
    if (l.id === id && l.kind === 'text') out = l;
  });
  return out;
}

export const textTool = new TextTool();
