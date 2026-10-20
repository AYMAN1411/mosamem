<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Compositor } from '../../engine/compositor';
  import { display } from '../../engine/display';
  import { app, viewportToDoc, nextGuideId, type Guide } from '../state.svelte';
  import { i18n } from '../i18n/index.svelte';
  import Rulers from './Rulers.svelte';
  import { SelectionInteraction, SELECTION_TOOLS } from './selectionTools';
  import { PaintInteraction, PAINT_TOOLS, settingsFor } from './paintTools';
  import { drawOverlay } from './overlay';
  import { transform } from './transform.svelte';
  import { crop } from './crop.svelte';
  import { nudge } from '../actions';

  let stage = $state<HTMLDivElement>();
  let gpuCanvas = $state<HTMLCanvasElement>();
  let overlay = $state<HTMLCanvasElement>();
  let compositor: Compositor | null = null;
  let dpr = $state(devicePixelRatio);
  let spaceDown = $state(false);
  let altDown = $state(false);
  /** Guide being dragged (new from a ruler, or an existing one). */
  let draggingGuide = $state<Guide | null>(null);
  let hoverGuide = $state<Guide | null>(null);
  /** Bumped whenever the overlay must be redrawn for an interaction in progress. */
  let overlayTick = $state(0);
  const selTool = new SelectionInteraction(() => overlayTick++);
  const paintTool = new PaintInteraction(() => overlayTick++);
  transform.setRedraw(() => overlayTick++);
  crop.setRedraw(() => overlayTick++);

  // the crop rectangle follows the document; switching tools drops a pending crop
  $effect(() => {
    if (app.tool === 'crop' && app.active) crop.ensure();
  });

  const doc = $derived(app.active);

  onMount(() => {
    compositor = new Compositor(gpuCanvas!);
    display.compositor = compositor;
    (window as unknown as { __lawha: unknown }).__lawha = { compositor, app, paintTool };
    const ro = new ResizeObserver(([e]) => {
      const { width, height } = e.contentRect;
      app.viewport = { w: width, h: height };
      dpr = devicePixelRatio;
      compositor!.resize(width, height, dpr);
      if (app.active?.fitted) app.fit();
    });
    ro.observe(stage!);
    const end = () => (gesture = null);
    stage!.addEventListener('gesturestart', onGestureStart);
    stage!.addEventListener('gesturechange', onGestureChange);
    stage!.addEventListener('gestureend', end);
    const mq = matchMedia(`(resolution: ${devicePixelRatio}dppx)`);
    const onDpr = () => (dpr = devicePixelRatio);
    mq.addEventListener('change', onDpr);
    return () => {
      ro.disconnect();
      mq.removeEventListener('change', onDpr);
      stage?.removeEventListener('gesturestart', onGestureStart);
      stage?.removeEventListener('gesturechange', onGestureChange);
      stage?.removeEventListener('gestureend', end);
    };
  });

  onDestroy(() => compositor?.destroy());

  // keep the compositor in sync with the active document and its view
  $effect(() => {
    if (!compositor) return;
    const d = doc;
    compositor.setDocument(d ? { id: d.info.id, width: d.info.width, height: d.info.height, version: d.info.version, selection: d.info.selectionKey } : null);
    if (d) compositor.setView({ zoom: d.view.zoom * dpr, cx: d.view.cx, cy: d.view.cy, rotation: (d.view.rotation * Math.PI) / 180 });
    compositor.pixelGrid = app.showPixelGrid;
    compositor.requestFrame();
  });

  // overlay: grid, guides, document edge
  $effect(() => {
    if (!overlay) return;
    const vp = app.viewport;
    overlay.width = Math.round(vp.w * dpr);
    overlay.height = Math.round(vp.h * dpr);
    const ctx = overlay.getContext('2d')!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, vp.w, vp.h);
    if (!doc) return;
    void overlayTick;
    drawOverlay(ctx, doc, vp, {
      grid: app.showGrid ? { spacing: app.gridSpacing, subdivisions: app.gridSubdivisions } : null,
      guides: app.showGuides ? [...doc.guides, ...(draggingGuide && !doc.guides.includes(draggingGuide) ? [draggingGuide] : [])] : [],
      hot: hoverGuide ?? draggingGuide,
    });
    selTool.draw(ctx);
    paintTool.draw(ctx);
    transform.draw(ctx);
    if (app.tool === 'crop') {
      void crop.rect;
      crop.draw(ctx);
    }
    void transform.active?.quad;
  });

  function local(e: { clientX: number; clientY: number }) {
    const r = stage!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  // ---- wheel / trackpad ----
  function onWheel(e: WheelEvent) {
    if (!doc) return;
    e.preventDefault();
    const p = local(e);
    if (e.ctrlKey || e.metaKey || e.altKey) {
      const k = e.deltaMode === 1 ? 0.05 : 0.0025;
      app.zoomTo(doc.view.zoom * Math.exp(-e.deltaY * k * (e.ctrlKey ? 2.5 : 1)), p);
    } else {
      app.panBy(-e.deltaX, -e.deltaY);
    }
  }

  // Safari/WKWebView trackpad gestures: pinch to zoom, two-finger twist to rotate
  let gesture: { zoom: number; rotation: number; at: { x: number; y: number } } | null = null;
  function onGestureStart(e: Event) {
    e.preventDefault();
    if (!doc) return;
    gesture = { zoom: doc.view.zoom, rotation: doc.view.rotation, at: local(e as unknown as MouseEvent) };
  }
  function onGestureChange(e: Event) {
    e.preventDefault();
    if (!gesture || !doc) return;
    const g = e as unknown as { scale: number; rotation: number };
    app.zoomTo(gesture.zoom * g.scale, gesture.at);
    if (app.tool === 'rotateView' && Math.abs(g.rotation) > 2) app.rotateTo(gesture.rotation + g.rotation);
  }

  // ---- pointer tools ----
  type Drag =
    | { kind: 'pan'; x: number; y: number }
    | { kind: 'zoom'; x: number; start: number; at: { x: number; y: number }; moved: boolean; out: boolean }
    | { kind: 'rotate'; startAngle: number; startRot: number }
    | { kind: 'guide' };
  let drag: Drag | null = null;

  const effectiveTool = $derived(spaceDown ? 'hand' : app.tool);

  function guideAt(p: { x: number; y: number }): Guide | null {
    if (!doc || !app.showGuides) return null;
    const dp = viewportToDoc(doc.view, app.viewport, p.x, p.y);
    const tol = 4 / doc.view.zoom;
    return doc.guides.find((g) => Math.abs((g.axis === 'h' ? dp.y : dp.x) - g.pos) <= tol) ?? null;
  }

  function onPointerDown(e: PointerEvent) {
    if (!doc || e.button > 1) return;
    try {
      stage!.setPointerCapture(e.pointerId);
    } catch {
      /* the pointer is gone already (or synthetic); keep going without capture */
    }
    const p = local(e);
    if (e.button === 1 || effectiveTool === 'hand') {
      drag = { kind: 'pan', x: e.clientX, y: e.clientY };
      return;
    }
    // a free transform takes every press until it is committed or cancelled
    if (transform.active?.free) {
      transform.down(e, p);
      return;
    }
    if (app.tool === 'move') {
      const g = guideAt(p);
      if (g) {
        draggingGuide = g;
        drag = { kind: 'guide' };
        return;
      }
      transform.down(e, p);
      return;
    }
    if (effectiveTool === 'crop') {
      crop.down(e, p);
      return;
    }
    if (SELECTION_TOOLS.includes(effectiveTool)) {
      void selTool.down(e, p);
      return;
    }
    if (PAINT_TOOLS.includes(effectiveTool)) {
      void paintTool.down(e, p);
      return;
    }
    if (effectiveTool === 'zoom') {
      drag = { kind: 'zoom', x: e.clientX, start: doc.view.zoom, at: p, moved: false, out: e.altKey };
    } else if (effectiveTool === 'rotateView') {
      const c = { x: app.viewport.w / 2, y: app.viewport.h / 2 };
      drag = { kind: 'rotate', startAngle: Math.atan2(p.y - c.y, p.x - c.x), startRot: doc.view.rotation };
    }
  }

  function onPointerMove(e: PointerEvent) {
    if (!doc) return;
    const p = local(e);
    if (!drag && (transform.busy || (app.tool === 'move' && !hoverGuide))) {
      transform.move(e, p);
      if (transform.busy) return;
    }
    if (!drag && effectiveTool === 'crop') {
      crop.move(e, p);
      return;
    }
    if (SELECTION_TOOLS.includes(effectiveTool) && !drag) {
      selTool.move(e, p);
      if (app.tool === 'quickSelect') overlayTick++;
      return;
    }
    if (PAINT_TOOLS.includes(effectiveTool) && !drag) {
      paintTool.move(e, p);
      return;
    }
    if (!drag) {
      hoverGuide = app.tool === 'move' ? guideAt(p) : null;
      return;
    }
    if (drag.kind === 'pan') {
      app.panBy(e.clientX - drag.x, e.clientY - drag.y);
      drag.x = e.clientX;
      drag.y = e.clientY;
    } else if (drag.kind === 'zoom') {
      const dx = e.clientX - drag.x;
      if (Math.abs(dx) > 3) drag.moved = true;
      if (drag.moved) app.zoomTo(drag.start * Math.exp(dx * 0.01), drag.at);
    } else if (drag.kind === 'rotate') {
      const c = { x: app.viewport.w / 2, y: app.viewport.h / 2 };
      let deg = drag.startRot + ((Math.atan2(p.y - c.y, p.x - c.x) - drag.startAngle) * 180) / Math.PI;
      if (e.shiftKey) deg = Math.round(deg / 15) * 15;
      app.rotateTo(deg);
    } else if (drag.kind === 'guide' && draggingGuide) {
      moveGuide(draggingGuide, e);
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (!drag && transform.busy) {
      transform.up();
      return;
    }
    if (!drag && effectiveTool === 'crop') {
      crop.up();
      return;
    }
    if (SELECTION_TOOLS.includes(effectiveTool) && !drag) {
      void selTool.up(e, local(e));
      return;
    }
    if (PAINT_TOOLS.includes(effectiveTool) && !drag) {
      void paintTool.up(e, local(e));
      return;
    }
    if (drag?.kind === 'zoom' && !drag.moved) app.zoomStep(drag.out || e.altKey ? -1 : 1, drag.at);
    if (drag?.kind === 'guide' && draggingGuide) finishGuide(e);
    drag = null;
  }

  // ---- guides dragged out of the rulers ----
  function moveGuide(g: Guide, e: PointerEvent) {
    const p = local(e);
    const dp = viewportToDoc(doc!.view, app.viewport, p.x, p.y);
    const raw = g.axis === 'h' ? dp.y : dp.x;
    g.pos = e.shiftKey ? Math.round(raw / 10) * 10 : Math.round(raw);
    draggingGuide = { ...g };
    if (doc!.guides.some((x) => x.id === g.id)) doc!.guides = doc!.guides.map((x) => (x.id === g.id ? { ...g } : x));
  }

  function finishGuide(e: PointerEvent) {
    const g = draggingGuide!;
    const r = stage!.getBoundingClientRect();
    const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
    const others = doc!.guides.filter((x) => x.id !== g.id);
    doc!.guides = inside ? [...others, { ...g }] : others; // dropping back on a ruler deletes it
    draggingGuide = null;
  }

  function startGuideFromRuler(axis: 'h' | 'v', e: PointerEvent) {
    if (!doc) return;
    app.showGuides = true;
    draggingGuide = { id: nextGuideId(), axis, pos: -1e6 };
    drag = { kind: 'guide' };
    stage!.setPointerCapture(e.pointerId);
  }

  // ---- keys that change the canvas tool temporarily ----
  function editable(t: EventTarget | null) {
    return t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement;
  }
  function onKey(e: KeyboardEvent, down: boolean) {
    if (down && !editable(e.target)) {
      if (transform.active?.free && (e.key === 'Enter' || e.key === 'Escape')) {
        e.preventDefault();
        e.stopPropagation();
        if (e.key === 'Enter') void transform.commit();
        else transform.cancel();
        return;
      }
      if (app.tool === 'crop' && (e.key === 'Enter' || e.key === 'Escape')) {
        e.preventDefault();
        e.stopPropagation();
        if (e.key === 'Enter') void crop.commit();
        else crop.reset();
        return;
      }
      if (app.tool === 'move' && !transform.busy && e.key.startsWith('Arrow') && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        e.stopPropagation();
        const k = e.shiftKey ? 10 : 1;
        void nudge(e.key === 'ArrowLeft' ? -k : e.key === 'ArrowRight' ? k : 0, e.key === 'ArrowUp' ? -k : e.key === 'ArrowDown' ? k : 0);
        return;
      }
    }
    if (down && !editable(e.target) && selTool.key(e)) {
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (e.code === 'Space' && !editable(e.target)) {
      spaceDown = down;
      if (down) e.preventDefault();
    }
    altDown = e.altKey;
  }

  const cursor = $derived.by(() => {
    if (drag?.kind === 'pan') return 'grabbing';
    if (transform.active) return transform.cursor() ?? 'default';
    if (effectiveTool === 'crop') return crop.cursor() ?? 'default';
    if (hoverGuide || draggingGuide) return (hoverGuide ?? draggingGuide)!.axis === 'h' ? 'row-resize' : 'col-resize';
    switch (effectiveTool) {
      case 'hand':
        return 'grab';
      case 'zoom':
        return altDown ? 'zoom-out' : 'zoom-in';
      case 'rotateView':
        return 'alias';
      case 'marqueeRect':
      case 'marqueeEllipse':
      case 'lasso':
      case 'lassoPolygon':
      case 'magicWand':
        return 'crosshair';
      case 'quickSelect':
        return 'none';
      case 'gradient':
      case 'eyedropper':
        return 'crosshair';
      case 'bucket':
        return 'cell';
      default:
        return settingsFor(effectiveTool) ? 'none' : 'default';
    }
  });
</script>

<svelte:window onkeydowncapture={(e) => onKey(e, true)} onkeyup={(e) => onKey(e, false)} onblur={() => (spaceDown = false)} />

<div class="canvas-area" class:no-rulers={!app.showRulers} dir={i18n.dir}>
  {#if app.showRulers}
    <Rulers {doc} viewport={app.viewport} onGuideStart={startGuideFromRuler} />
  {/if}
  <div
    class="stage"
    bind:this={stage}
    style:cursor
    onwheel={onWheel}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={() => {
      drag = null;
      selTool.cancel();
    }}
    onpointerleave={() => {
      selTool.hover = null;
      paintTool.hover = null;
      overlayTick++;
    }}
    role="application"
  >
    <canvas class="gpu" bind:this={gpuCanvas}></canvas>
    <canvas class="overlay" bind:this={overlay}></canvas>
  </div>
</div>

<style>
  .canvas-area { position: relative; display: grid; grid-template-columns: var(--ruler) minmax(0, 1fr); grid-template-rows: var(--ruler) minmax(0, 1fr); height: 100%; min-height: 0; min-width: 0; background: var(--bg-0); }
  .canvas-area.no-rulers { grid-template-columns: 0 minmax(0, 1fr); grid-template-rows: 0 minmax(0, 1fr); }
  .stage { position: relative; grid-column: 2; grid-row: 2; overflow: hidden; touch-action: none; }
  canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
  .overlay { pointer-events: none; }
</style>
