<script lang="ts">
  // Rulers along the top and the start side of the canvas, in document pixels. They follow the view
  // as if it were unrotated; with a rotated view they show the axes through the view center.
  import { viewportToDoc, type DocState } from '../state.svelte';

  let { doc, viewport, onGuideStart }: { doc: DocState | null; viewport: { w: number; h: number }; onGuideStart: (axis: 'h' | 'v', e: PointerEvent) => void } = $props();

  let top = $state<HTMLCanvasElement>();
  let side = $state<HTMLCanvasElement>();
  const R = 20;

  function step(zoom: number): number {
    const target = 60 / zoom; // ~60 css px between labels
    const p = Math.pow(10, Math.floor(Math.log10(target)));
    for (const m of [1, 2, 5, 10]) if (p * m >= target) return p * m;
    return p * 10;
  }

  function draw(canvas: HTMLCanvasElement, horizontal: boolean) {
    const dpr = devicePixelRatio;
    const len = horizontal ? viewport.w : viewport.h;
    canvas.width = Math.round((horizontal ? len : R) * dpr);
    canvas.height = Math.round((horizontal ? R : len) * dpr);
    const ctx = canvas.getContext('2d')!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(document.documentElement);
    ctx.fillStyle = css.getPropertyValue('--bg-1');
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.strokeStyle = css.getPropertyValue('--line-strong');
    ctx.fillStyle = css.getPropertyValue('--fg-2');
    ctx.font = '10px "IBM Plex Sans Arabic", sans-serif';
    ctx.direction = 'ltr'; // numbers (including negatives) always read left to right
    ctx.beginPath();
    // edge line toward the canvas
    if (horizontal) {
      ctx.moveTo(0, R - 0.5);
      ctx.lineTo(len, R - 0.5);
    } else {
      const x = document.documentElement.dir === 'rtl' ? 0.5 : R - 0.5;
      ctx.moveTo(x, 0);
      ctx.lineTo(x, len);
    }
    if (doc) {
      const z = doc.view.zoom;
      const origin = horizontal ? viewportToDoc({ ...doc.view, rotation: 0 }, viewport, 0, 0).x : viewportToDoc({ ...doc.view, rotation: 0 }, viewport, 0, 0).y;
      const big = step(z);
      const m = Math.round(big / Math.pow(10, Math.floor(Math.log10(big) + 1e-9)));
      let sub = m === 5 ? 5 : m === 2 ? 4 : 10;
      if ((big / sub) * z < 4) sub = 2;
      const small = big / sub;
      const first = Math.floor(origin / small) * small;
      const rtl = document.documentElement.dir === 'rtl';
      for (let v = first; (v - origin) * z <= len; v += small) {
        const pos = Math.round((v - origin) * z) + 0.5;
        const isBig = Math.abs(v / big - Math.round(v / big)) < 1e-6;
        const tick = isBig ? R : R * 0.3;
        if (horizontal) {
          ctx.moveTo(pos, R);
          ctx.lineTo(pos, R - tick);
          if (isBig) ctx.fillText(String(Math.round(v)), pos + 3, 10);
        } else {
          const x0 = rtl ? 0 : R;
          ctx.moveTo(x0, pos);
          ctx.lineTo(rtl ? tick : R - tick, pos);
          if (isBig) {
            ctx.save();
            ctx.translate(rtl ? 14 : 10, pos + 3);
            ctx.rotate(-Math.PI / 2);
            ctx.textAlign = 'right';
            ctx.fillText(String(Math.round(v)), 0, 0);
            ctx.restore();
          }
        }
      }
    }
    ctx.stroke();
  }

  $effect(() => {
    void doc?.view;
    void viewport;
    if (top) draw(top, true);
    if (side) draw(side, false);
  });
</script>

<div class="corner"></div>
<div class="top"><canvas bind:this={top} onpointerdown={(e) => onGuideStart('h', e)}></canvas></div>
<div class="side"><canvas bind:this={side} onpointerdown={(e) => onGuideStart('v', e)}></canvas></div>

<style>
  .corner { grid-column: 1; grid-row: 1; background: var(--bg-1); border-bottom: 1px solid var(--line-strong); }
  .top, .side { position: relative; overflow: hidden; min-width: 0; min-height: 0; }
  .top { grid-column: 2; grid-row: 1; }
  .side { grid-column: 1; grid-row: 2; }
  canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; cursor: default; }
</style>
