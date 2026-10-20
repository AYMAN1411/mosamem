import { describe, expect, it } from 'vitest';
import { app, docToViewport, viewportToDoc, type ViewState } from '../app/state.svelte';
import type { DocInfo } from '../engine/bridge';

const vp = { w: 1000, h: 700 };

describe('view transform', () => {
  it('viewport ↔ document are inverses at any zoom and rotation', () => {
    for (const v of [
      { zoom: 1, cx: 500, cy: 300, rotation: 0 },
      { zoom: 0.137, cx: 3000, cy: 2000, rotation: 33 },
      { zoom: 8, cx: 12.5, cy: 7.25, rotation: -170 },
    ] satisfies ViewState[]) {
      for (const [x, y] of [[0, 0], [1000, 700], [123.4, 567.8]]) {
        const d = viewportToDoc(v, vp, x, y);
        const back = docToViewport(v, vp, d.x, d.y);
        expect(back.x).toBeCloseTo(x, 6);
        expect(back.y).toBeCloseTo(y, 6);
      }
    }
  });

  it('the view center maps to the viewport center', () => {
    const p = docToViewport({ zoom: 3, cx: 10, cy: 20, rotation: 45 }, vp, 10, 20);
    expect(p).toEqual({ x: 500, y: 350 });
  });
});

describe('zooming', () => {
  const info: DocInfo = { id: 1, name: 't', path: null, width: 6000, height: 4000, ppi: 72, layers: [], active: 0, version: 1, dirty: false, history: { labels: [], current: 0 } };

  it('fit shows the whole document inside the viewport', () => {
    app.viewport = vp;
    app.addDocument(info);
    const d = app.active!;
    expect(d.info.width * d.view.zoom).toBeLessThanOrEqual(vp.w);
    expect(d.info.height * d.view.zoom).toBeLessThanOrEqual(vp.h);
    expect(d.fitted).toBe(true);
  });

  it('zooming about a point keeps that document point under the cursor', () => {
    const d = app.active!;
    d.view = { ...d.view, rotation: 20 };
    const at = { x: 812, y: 133 };
    const before = viewportToDoc(d.view, vp, at.x, at.y);
    app.zoomTo(2.5, at);
    const after = viewportToDoc(app.active!.view, vp, at.x, at.y);
    expect(after.x).toBeCloseTo(before.x, 6);
    expect(after.y).toBeCloseTo(before.y, 6);
    expect(app.active!.fitted).toBe(false);
  });

  it('zoom steps move to the next preset and clamp at the limits', () => {
    app.zoomTo(1);
    app.zoomStep(1);
    expect(app.active!.view.zoom).toBe(2);
    app.zoomStep(-1);
    app.zoomStep(-1);
    expect(app.active!.view.zoom).toBeCloseTo(0.6667, 4);
    app.zoomTo(1000);
    expect(app.active!.view.zoom).toBe(64);
  });

  it('panning moves the content with the pointer, also when rotated', () => {
    const d = app.active!;
    d.view = { zoom: 2, cx: 3000, cy: 2000, rotation: 90 };
    const p = docToViewport(d.view, vp, 3100, 2100);
    app.panBy(15, -40);
    const q = docToViewport(app.active!.view, vp, 3100, 2100);
    expect(q.x - p.x).toBeCloseTo(15, 6);
    expect(q.y - p.y).toBeCloseTo(-40, 6);
  });

  it('rotation is normalized to (-180, 180]', () => {
    app.rotateTo(270);
    expect(app.active!.view.rotation).toBe(-90);
    app.rotateTo(-180);
    expect(app.active!.view.rotation).toBe(180);
  });
});
