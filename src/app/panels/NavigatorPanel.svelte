<script lang="ts">
  // Thumbnail of the whole document with the visible area outlined; click or drag to move the view.
  import { app, viewportToDoc } from '../state.svelte';
  import { tileUrl } from '../../engine/bridge';
  import { t } from '../i18n/index.svelte';

  const TILE = 256;
  let box = $state<HTMLDivElement>();
  let canvas = $state<HTMLCanvasElement>();
  let size = $state({ w: 240, h: 160 });
  let loaded = '';

  const doc = $derived(app.active);
  const scale = $derived(doc ? Math.min(size.w / doc.info.width, size.h / doc.info.height) : 1);

  $effect(() => {
    if (!box) return;
    const ro = new ResizeObserver(([e]) => (size = { w: Math.max(80, e.contentRect.width - 16), h: Math.max(80, Math.min(220, e.contentRect.width * 0.7)) }));
    ro.observe(box);
    return () => ro.disconnect();
  });

  // Loads the coarse tiles that cover the whole document into the thumbnail.
  $effect(() => {
    if (!doc || !canvas) return;
    const info = doc.info;
    const key = `${info.id}/${info.version}/${scale.toFixed(4)}`;
    if (key === loaded) return;
    loaded = key;
    const cw = Math.max(1, Math.round(info.width * scale));
    const ch = Math.max(1, Math.round(info.height * scale));
    canvas.width = cw * devicePixelRatio;
    canvas.height = ch * devicePixelRatio;
    canvas.style.width = `${cw}px`;
    canvas.style.height = `${ch}px`;
    const ctx = canvas.getContext('2d')!;
    const px = scale * devicePixelRatio; // device px per doc px
    let lod = 0;
    while (lod < 6 && px * (1 << (lod + 1)) <= 1) lod++;
    const span = TILE << lod;
    for (let ty = 0; ty * span < info.height; ty++)
      for (let tx = 0; tx * span < info.width; tx++) {
        fetch(tileUrl(info.id, info.version, lod, tx, ty)).then(async (r) => {
          if (r.status !== 200 || loaded !== key) return;
          const data = new Uint8ClampedArray(await r.arrayBuffer());
          const bmp = await createImageBitmap(new ImageData(data, TILE, TILE));
          ctx.imageSmoothingQuality = 'high';
          ctx.drawImage(bmp, tx * span * px, ty * span * px, span * px, span * px);
        });
      }
  });

  const viewRect = $derived.by(() => {
    if (!doc) return null;
    const vp = app.viewport;
    const pts = [
      [0, 0],
      [vp.w, 0],
      [vp.w, vp.h],
      [0, vp.h],
    ].map(([x, y]) => viewportToDoc(doc.view, vp, x, y));
    return pts.map((p) => `${p.x * scale},${p.y * scale}`).join(' ');
  });

  function moveTo(e: PointerEvent) {
    if (!doc) return;
    const r = canvas!.getBoundingClientRect();
    doc.view = { ...doc.view, cx: (e.clientX - r.left) / scale, cy: (e.clientY - r.top) / scale };
    doc.fitted = false;
  }

  function down(e: PointerEvent) {
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    moveTo(e);
  }
</script>

<div class="nav" bind:this={box}>
  {#if doc}
    <div class="frame" onpointerdown={down} onpointermove={(e) => e.buttons && moveTo(e)} role="presentation">
      <canvas bind:this={canvas}></canvas>
      <svg class="view" width={doc.info.width * scale} height={doc.info.height * scale}>
        <polygon points={viewRect} />
      </svg>
    </div>
  {:else}
    <div class="empty">{t('panel.empty')}</div>
  {/if}
</div>

<style>
  .nav { padding: 8px; }
  .frame { position: relative; width: fit-content; margin: 0 auto; background: repeating-conic-gradient(#555 0 25%, #777 0 50%) 0 0 / 8px 8px; direction: ltr; }
  canvas { display: block; }
  .view { position: absolute; inset: 0; overflow: visible; pointer-events: none; }
  polygon { fill: none; stroke: var(--danger); stroke-width: 1.5; }
  .empty { padding: 8px; color: var(--fg-3); }
</style>
