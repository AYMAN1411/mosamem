<script lang="ts">
  import type { Adjustment, Hsl } from '../../../engine/adjust';
  import { t, type Key } from '../../i18n/index.svelte';
  import Slider from './Slider.svelte';

  type H = Extract<Adjustment, { kind: 'hueSaturation' }>;
  let { value, onchange }: { value: H; onchange: (next: H, live: boolean) => void } = $props();

  const RANGES = ['master', 'reds', 'yellows', 'greens', 'cyans', 'blues', 'magentas'] as const;
  let range = $state(0);
  const cur = $derived(value.colorize ? value.colorizeHsl : range === 0 ? value.master : value.ranges[range - 1]);

  function set(patch: Partial<Hsl>, live: boolean) {
    if (value.colorize) return onchange({ ...value, colorizeHsl: { ...value.colorizeHsl, ...patch } }, live);
    if (range === 0) return onchange({ ...value, master: { ...value.master, ...patch } }, live);
    onchange({ ...value, ranges: value.ranges.map((r, i) => (i === range - 1 ? { ...r, ...patch } : r)) }, live);
  }

  const RAINBOW = 'linear-gradient(90deg, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)';
  // the second bar shows where each hue goes (master or colorize hue)
  const shifted = $derived.by(() => {
    const h = value.colorize ? null : value.master.hue;
    if (h == null) return `hsl(${value.colorizeHsl.hue} ${value.colorizeHsl.saturation}% 50%)`;
    const stops = [0, 60, 120, 180, 240, 300, 360].map((d) => `hsl(${d + h} 100% 50%)`);
    return `linear-gradient(90deg, ${stops.join(', ')})`;
  });
</script>

<div class="hs">
  {#if !value.colorize}
    <select class="field" bind:value={range} aria-label={t('adj.tone')}>
      {#each RANGES as r, i (r)}<option value={i}>{t(`adj.range.${r}` as Key)}</option>{/each}
    </select>
  {/if}
  <Slider label={t('adj.hue')} value={cur.hue} min={value.colorize ? 0 : -180} max={value.colorize ? 360 : 180} onchange={(v, live) => set({ hue: v }, live)} />
  <Slider label={t('adj.saturation')} value={cur.saturation} min={value.colorize ? 0 : -100} max={100} onchange={(v, live) => set({ saturation: v }, live)} />
  <Slider label={t('adj.lightness')} value={cur.lightness} min={-100} max={100} track="linear-gradient(90deg, #000, #808080, #fff)" onchange={(v, live) => set({ lightness: v }, live)} />
  <label class="check"><input type="checkbox" checked={value.colorize} onchange={(e) => onchange({ ...value, colorize: e.currentTarget.checked }, false)} />{t('adj.colorize')}</label>
  <div class="bars" dir="ltr">
    <span style:background={RAINBOW}></span>
    <span style:background={shifted}></span>
  </div>
</div>

<style>
  .hs { display: flex; flex-direction: column; gap: 8px; }
  .check { display: flex; align-items: center; gap: 6px; color: var(--fg-1); font-size: var(--fs-1); }
  .bars { display: flex; flex-direction: column; gap: 2px; }
  .bars span { height: 8px; border-radius: 2px; }
</style>
