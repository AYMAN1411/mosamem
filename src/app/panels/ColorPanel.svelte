<script lang="ts">
  // HSB picker: a saturation/brightness square and a hue strip, plus hex and RGB fields, for the
  // foreground or background color.
  import { app } from '../state.svelte';
  import { t } from '../i18n/index.svelte';
  import Icon from '../icons/Icon.svelte';

  let which = $state<'fg' | 'bg'>('fg');
  const current = $derived(which === 'fg' ? app.fg : app.bg);

  function set(hex: string) {
    if (which === 'fg') app.fg = hex;
    else app.bg = hex;
  }

  const rgb = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const hex = (r: number, g: number, b: number) => '#' + [r, g, b].map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, '0')).join('');

  function toHsv([r, g, b]: number[]) {
    const R = r / 255, G = g / 255, B = b / 255;
    const max = Math.max(R, G, B), min = Math.min(R, G, B), d = max - min;
    let h = 0;
    if (d) h = max === R ? ((G - B) / d) % 6 : max === G ? (B - R) / d + 2 : (R - G) / d + 4;
    return { h: ((h * 60) + 360) % 360, s: max ? d / max : 0, v: max };
  }
  function fromHsv(h: number, s: number, v: number) {
    const f = (n: number) => {
      const k = (n + h / 60) % 6;
      return 255 * (v - v * s * Math.max(0, Math.min(k, 4 - k, 1)));
    };
    return hex(f(5), f(3), f(1));
  }

  // hue is kept separately so it doesn't jump when the color goes gray
  let hue = $state(toHsv(rgb(app.fg)).h);
  const hsv = $derived(toHsv(rgb(current)));
  $effect(() => {
    if (hsv.s > 0.001 && hsv.v > 0.001) hue = hsv.h;
  });

  function drag(e: PointerEvent, f: (x: number, y: number) => void) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const go = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      f(Math.max(0, Math.min(1, (ev.clientX - r.left) / r.width)), Math.max(0, Math.min(1, (ev.clientY - r.top) / r.height)));
    };
    go(e);
    const move = (ev: PointerEvent) => go(ev);
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', () => el.removeEventListener('pointermove', move), { once: true });
  }

  const sv = (e: PointerEvent) => drag(e, (x, y) => set(fromHsv(hue, x, 1 - y)));
  const hueDrag = (e: PointerEvent) => drag(e, (x) => {
    hue = x * 360;
    set(fromHsv(hue, hsv.s, hsv.v));
  });
  const valid = (v: string) => (/^#?[0-9a-f]{6}$/i.test(v) ? (v.startsWith('#') ? v : `#${v}`).toLowerCase() : null);
</script>

<div class="color">
  <div class="top">
    <div class="swatches">
      <button class="sw fg" class:on={which === 'fg'} style:background={app.fg} title={t('tool.foreground')} onclick={() => (which = 'fg')} aria-label={t('tool.foreground')}></button>
      <button class="sw bg" class:on={which === 'bg'} style:background={app.bg} title={t('tool.background')} onclick={() => (which = 'bg')} aria-label={t('tool.background')}></button>
    </div>
    <div class="tools">
      <button class="icon-btn" title={t('tool.swapColors')} onclick={() => ([app.fg, app.bg] = [app.bg, app.fg])}><Icon name="swap" /></button>
      <button class="icon-btn" title={t('tool.defaultColors')} onclick={() => { app.fg = '#000000'; app.bg = '#ffffff'; }}><Icon name="defaultColors" /></button>
    </div>
  </div>
  <div class="sv" style:background-color={fromHsv(hue, 1, 1)} onpointerdown={sv} role="slider" aria-label={t('color.picker')} aria-valuenow={Math.round(hsv.s * 100)} tabindex="-1">
    <span class="knob" style:left={`${hsv.s * 100}%`} style:top={`${(1 - hsv.v) * 100}%`}></span>
  </div>
  <div class="hue" onpointerdown={hueDrag} role="slider" aria-valuenow={Math.round(hue)} tabindex="-1">
    <span class="hknob" style:left={`${(hue / 360) * 100}%`}></span>
  </div>
  <div class="fields">
    <input class="field num hex" value={current} onchange={(e) => { const v = valid(e.currentTarget.value); if (v) set(v); }} aria-label="hex" />
    {#each ['R', 'G', 'B'] as ch, i (ch)}
      <label class="rgb"><span>{ch}</span><input class="field num" type="number" min="0" max="255" value={rgb(current)[i]} onchange={(e) => { const c = rgb(current); c[i] = Number(e.currentTarget.value); set(hex(c[0], c[1], c[2])); }} /></label>
    {/each}
  </div>
</div>

<style>
  .color { padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; }
  .top { display: flex; align-items: center; justify-content: space-between; }
  .swatches { position: relative; width: 52px; height: 40px; }
  .sw { position: absolute; width: 30px; height: 30px; border-radius: 4px; border: 2px solid var(--bg-0); box-shadow: 0 0 0 1px var(--line-strong); padding: 0; }
  .sw.on { box-shadow: 0 0 0 2px var(--accent); }
  .fg { top: 0; inset-inline-start: 0; z-index: 1; }
  .bg { bottom: 0; inset-inline-end: 0; }
  .tools { display: flex; gap: 2px; }
  .sv { position: relative; height: 120px; border-radius: 4px; background-image: linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, transparent); cursor: crosshair; touch-action: none; direction: ltr; }
  .knob { position: absolute; width: 12px; height: 12px; margin: -6px 0 0 -6px; border-radius: 50%; border: 2px solid #fff; box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.6); pointer-events: none; }
  .hue { position: relative; height: 12px; border-radius: 6px; background: linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00); touch-action: none; direction: ltr; }
  .hknob { position: absolute; top: -2px; width: 8px; height: 16px; margin-left: -4px; border-radius: 3px; border: 2px solid #fff; box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.6); pointer-events: none; }
  .fields { display: flex; gap: 6px; }
  .hex { width: 84px; }
  .rgb { display: flex; align-items: center; gap: 3px; color: var(--fg-2); font-size: var(--fs-0); }
  .rgb input { width: 46px; padding: 0 4px; }
</style>
