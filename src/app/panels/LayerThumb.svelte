<script lang="ts">
  import { thumbUrl, maskThumbUrl } from '../../engine/bridge';

  let { doc, layer, thumbKey, size = 34, mask = false }: { doc: number; layer: number; thumbKey: number; size?: number; mask?: boolean } = $props();
  let canvas = $state<HTMLCanvasElement>();

  $effect(() => {
    const c = canvas;
    if (!c) return;
    const px = Math.round(size * devicePixelRatio);
    let cancelled = false;
    fetch((mask ? maskThumbUrl : thumbUrl)(doc, layer, thumbKey, px)).then(async (r) => {
      if (cancelled || r.status !== 200) return;
      const buf = new Uint8Array(await r.arrayBuffer());
      const w = buf[0] | (buf[1] << 8);
      const h = buf[2] | (buf[3] << 8);
      c.width = w;
      c.height = h;
      c.style.width = `${w / devicePixelRatio}px`;
      c.style.height = `${h / devicePixelRatio}px`;
      c.getContext('2d')!.putImageData(new ImageData(new Uint8ClampedArray(buf.buffer, 4, w * h * 4), w, h), 0, 0);
    });
    return () => (cancelled = true);
  });
</script>

<span class="thumb" style:width={`${size}px`} style:height={`${size}px`}><canvas bind:this={canvas}></canvas></span>

<style>
  .thumb { display: grid; place-items: center; flex: none; border-radius: 3px; overflow: hidden; }
  canvas { display: block; background: repeating-conic-gradient(#666 0 25%, #8a8a8a 0 50%) 0 0 / 6px 6px; box-shadow: 0 0 0 1px var(--line-strong); }
</style>
