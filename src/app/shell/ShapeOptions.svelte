<script lang="ts">
  import { app } from '../state.svelte';
  import { shapeTool, KIND_OF, activeShape, type Dash } from '../canvas/shapes.svelte';
  import { t, fmt, type Key } from '../i18n/index.svelte';
  import type { StrokeAlign } from '../../engine/vector';

  // the active shape layer's kind decides the shape-specific fields; otherwise the tool's
  const o = $derived((void app.active?.info.version, shapeTool.current()));
  const kind = $derived(activeShape()?.shape?.shape.kind ?? KIND_OF[app.tool]);
  const line = $derived(kind === 'line');
  const num = (e: Event & { currentTarget: HTMLInputElement }) => parseFloat(e.currentTarget.value.replace(/[٫,]/, '.'));
</script>

{#if !line}
  <label class="inline" title={t('shape.fill')}>
    <input type="checkbox" checked={o.fillOn} onchange={(e) => shapeTool.set({ fillOn: e.currentTarget.checked })} />{t('shape.fill')}
    <input class="swatch" type="color" value={o.fill} disabled={!o.fillOn} onchange={(e) => shapeTool.set({ fill: e.currentTarget.value, fillOn: true })} />
  </label>
  <label class="inline" title={t('shape.stroke')}>
    <input type="checkbox" checked={o.strokeOn} onchange={(e) => shapeTool.set({ strokeOn: e.currentTarget.checked })} />{t('shape.stroke')}
    <input class="swatch" type="color" value={o.stroke} disabled={!o.strokeOn} onchange={(e) => shapeTool.set({ stroke: e.currentTarget.value, strokeOn: true })} />
  </label>
{:else}
  <label class="inline" title={t('shape.color')}>{t('shape.color')}
    <input class="swatch" type="color" value={o.stroke} onchange={(e) => shapeTool.set({ stroke: e.currentTarget.value })} />
  </label>
{/if}
<label class="inline" title={line ? t('shape.weight') : t('shape.width')}>{line ? t('shape.weight') : t('shape.width')}
  <input class="field num" value={fmt(o.width, 1)} onchange={(e) => num(e) >= 0 && shapeTool.set({ width: num(e) })} />px
</label>
{#if !line}
  <select class="field" value={o.align} title={t('shape.align')} onchange={(e) => shapeTool.set({ align: e.currentTarget.value as StrokeAlign })}>
    {#each ['inside', 'center', 'outside'] as a (a)}<option value={a} selected={a === o.align}>{t(`shape.align.${a}` as Key)}</option>{/each}
  </select>
{/if}
<select class="field" value={o.dash} title={t('shape.dash')} onchange={(e) => shapeTool.set({ dash: e.currentTarget.value as Dash })}>
  {#each ['solid', 'dash', 'dot'] as d (d)}<option value={d} selected={d === o.dash}>{t(`shape.dash.${d}` as Key)}</option>{/each}
</select>
<div class="sep"></div>
{#if kind === 'rect'}
  <label class="inline" title={t('shape.radius')}>{t('shape.radius')}<input class="field num" value={fmt(o.radius, 1)} onchange={(e) => num(e) >= 0 && shapeTool.set({ radius: num(e) })} />px</label>
{:else if kind === 'polygon'}
  <label class="inline" title={t('shape.sides')}>{t('shape.sides')}<input class="field num" type="number" min="3" max="100" value={o.sides} onchange={(e) => num(e) >= 3 && shapeTool.set({ sides: num(e) })} /></label>
{:else if kind === 'star'}
  <label class="inline" title={t('shape.points')}>{t('shape.points')}<input class="field num" type="number" min="3" max="100" value={o.points} onchange={(e) => num(e) >= 3 && shapeTool.set({ points: num(e) })} /></label>
  <label class="inline" title={t('shape.inner')}>{t('shape.inner')}<input class="field num" value={fmt(o.inner * 100)} onchange={(e) => num(e) > 0 && shapeTool.set({ inner: num(e) / 100 })} />%</label>
{:else if kind === 'line'}
  <label class="inline"><input type="checkbox" checked={o.arrowStart} onchange={(e) => shapeTool.set({ arrowStart: e.currentTarget.checked })} />{t('shape.arrowStart')}</label>
  <label class="inline"><input type="checkbox" checked={o.arrowEnd} onchange={(e) => shapeTool.set({ arrowEnd: e.currentTarget.checked })} />{t('shape.arrowEnd')}</label>
{/if}

<style>
  .inline { display: flex; align-items: center; gap: 4px; color: var(--fg-2); font-size: var(--fs-0); }
  .inline input.field { width: 48px; height: 24px; text-align: center; }
  .swatch { width: 28px; height: 24px; padding: 0; border: 1px solid var(--line-strong); border-radius: var(--radius-sm); background: none; }
  .swatch:disabled { opacity: 0.35; }
  select.field { height: 26px; max-width: 120px; }
</style>
