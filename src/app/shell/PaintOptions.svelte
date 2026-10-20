<script lang="ts">
  import { app } from '../state.svelte';
  import { t, fmt, type Key } from '../i18n/index.svelte';
  import { settingsFor } from '../canvas/paintTools';
  import { findLayer } from '../../engine/bridge';

  const s = $derived(settingsFor(app.tool));
  const kinds = ['linear', 'radial', 'angle', 'reflected', 'diamond'] as const;
  const sizes = [1, 3, 5, 11, 31, 51, 101];
  const onMask = $derived(!!app.active && app.maskTarget && !!findLayer(app.active.info.layers, app.active.info.active)?.mask);
  const pct = (v: number) => `${fmt(v * 100)}%`;
  const setPct = (e: Event, f: (v: number) => void) => {
    const n = parseFloat((e.currentTarget as HTMLInputElement).value.replace(/[%٪]/, ''));
    if (!isNaN(n)) f(Math.max(0, Math.min(100, n)) / 100);
  };
</script>

{#if s}
  <label class="inline">{t('opt.size')}<input class="field num small" type="number" min="1" max="5000" bind:value={s.diameter} />px</label>
  <label class="inline">{t('opt.hardness')}<input class="field num small" value={pct(s.hardness)} onchange={(e) => setPct(e, (v) => (s.hardness = v))} /></label>
  <div class="sep"></div>
  <label class="inline">{t('opt.opacity')}<input class="field num small" value={pct(s.opacity)} onchange={(e) => setPct(e, (v) => (s.opacity = v))} /></label>
  <label class="inline">{t('opt.flow')}<input class="field num small" value={pct(s.flow)} onchange={(e) => setPct(e, (v) => (s.flow = v))} /></label>
  <label class="inline">{t('opt.smoothing')}<input class="field num small" value={pct(s.smoothing)} onchange={(e) => setPct(e, (v) => (s.smoothing = v))} /></label>
  <button class="icon-btn" title={t('opt.pressureSize')} aria-pressed={s.pressureSize} onclick={() => (s.pressureSize = !s.pressureSize)}><span class="pen">⊙</span></button>
  <button class="icon-btn" title={t('opt.pressureOpacity')} aria-pressed={s.pressureOpacity} onclick={() => (s.pressureOpacity = !s.pressureOpacity)}><span class="pen">◐</span></button>
  {#if app.tool === 'clone'}
    <div class="sep"></div>
    <label class="check"><input type="checkbox" bind:checked={app.paint.clone.aligned} />{t('opt.aligned')}</label>
    <label class="check"><input type="checkbox" bind:checked={app.paint.clone.sampleAll} />{t('opt.sampleAll')}</label>
  {:else if app.tool === 'heal' || app.tool === 'spotHeal'}
    <div class="sep"></div>
    <label class="check"><input type="checkbox" bind:checked={app.paint.heal.sampleAll} />{t('opt.sampleAll')}</label>
  {/if}
  {#if onMask}<span class="mask-badge">{t('opt.mask')}</span>{/if}
{:else if app.tool === 'gradient'}
  {@const g = app.paint.gradient}
  <div class="seg" role="group">
    {#each kinds as k (k)}
      <button class="btn kind" aria-pressed={g.kind === k} onclick={() => (app.paint.gradient.kind = k)}>{t(`grad.${k}` as Key)}</button>
    {/each}
  </div>
  <label class="inline">{t('opt.opacity')}<input class="field num small" value={pct(g.opacity)} onchange={(e) => setPct(e, (v) => (app.paint.gradient.opacity = v))} /></label>
  <label class="check"><input type="checkbox" bind:checked={app.paint.gradient.reverse} />{t('opt.reverse')}</label>
  <label class="check"><input type="checkbox" bind:checked={app.paint.gradient.dither} />{t('opt.dither')}</label>
  <label class="check"><input type="checkbox" bind:checked={app.paint.gradient.toTransparent} />{t('opt.toTransparent')}</label>
{:else if app.tool === 'bucket'}
  <label class="inline">{t('opt.opacity')}<input class="field num small" value={pct(app.paint.bucket.opacity)} onchange={(e) => setPct(e, (v) => (app.paint.bucket.opacity = v))} /></label>
  <label class="inline">{t('opt.tolerance')}<input class="field num small" type="number" min="0" max="255" bind:value={app.paint.bucket.tolerance} /></label>
  <label class="check"><input type="checkbox" bind:checked={app.paint.bucket.antialias} />{t('opt.antialias')}</label>
  <label class="check"><input type="checkbox" bind:checked={app.paint.bucket.contiguous} />{t('opt.contiguous')}</label>
  <label class="check"><input type="checkbox" bind:checked={app.paint.bucket.allLayers} />{t('opt.allLayers')}</label>
{:else if app.tool === 'eyedropper'}
  <label class="inline">{t('opt.sampleSize')}
    <select class="field" bind:value={app.paint.eyedropper.size}>
      {#each sizes as n (n)}<option value={n}>{n === 1 ? t('opt.sample1') : t('opt.sampleN', { n })}</option>{/each}
    </select>
  </label>
  <label class="check"><input type="checkbox" bind:checked={app.paint.eyedropper.allLayers} />{t('opt.allLayers')}</label>
{/if}

<style>
  .inline { display: flex; align-items: center; gap: 6px; color: var(--fg-2); }
  .small { width: 58px; height: 24px; }
  .check { color: var(--fg-1); }
  .pen { font-size: 15px; }
  .seg { display: flex; gap: 2px; }
  .kind { height: 24px; padding: 0 8px; font-size: var(--fs-0); }
  .kind[aria-pressed='true'] { border-color: var(--accent); color: var(--accent); }
  .mask-badge { padding: 2px 8px; border-radius: 10px; background: var(--accent-soft); color: var(--accent); font-size: var(--fs-0); }
  select.field { height: 24px; }
</style>
