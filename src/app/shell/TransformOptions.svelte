<script lang="ts">
  // Move tool and Free Transform: position, size and angle while transforming, the reference
  // point, the interpolation, and commit/cancel.
  import { transform } from '../canvas/transform.svelte';
  import { t, fmt, type Key } from '../i18n/index.svelte';
  import Icon from '../icons/Icon.svelte';
  import type { ResampleMethod } from '../../engine/bridge';

  const r = $derived(transform.active ? (void transform.active.quad, transform.readout()) : null);
  const methods: ResampleMethod[] = ['nearest', 'bilinear', 'bicubic', 'lanczos'];
  const num = (e: Event & { currentTarget: HTMLInputElement }) => parseFloat(e.currentTarget.value.replace(/[٫,]/, '.'));
</script>

{#if transform.active?.free && r}
  <div class="refgrid" role="radiogroup" aria-label={t('xf.ref')} title={t('xf.ref')}>
    {#each [0, 1, 2, 3, 4, 5, 6, 7, 8] as i (i)}
      <button class="dot" class:on={transform.ref === i} onclick={() => (transform.ref = i)} aria-label={`${i}`}></button>
    {/each}
  </div>
  <label class="inline">X<input class="field num" value={fmt(r.x, 1)} onchange={(e) => transform.setNumbers({ x: num(e) })} /></label>
  <label class="inline">Y<input class="field num" value={fmt(r.y, 1)} onchange={(e) => transform.setNumbers({ y: num(e) })} /></label>
  <label class="inline">W<input class="field num" value={`${fmt(r.w, 1)}%`} onchange={(e) => transform.setNumbers({ w: num(e) })} /></label>
  <label class="inline">H<input class="field num" value={`${fmt(r.h, 1)}%`} onchange={(e) => transform.setNumbers({ h: num(e) })} /></label>
  <label class="inline"><Icon name="rotateView" size={15} /><input class="field num" value={`${fmt(r.angle, 1)}°`} onchange={(e) => transform.setNumbers({ angle: num(e) })} /></label>
  <label class="inline">{t('xf.method')}
    <select class="field" bind:value={transform.method}>
      {#each methods as m (m)}<option value={m}>{t(`method.${m}` as Key)}</option>{/each}
    </select>
  </label>
  <button class="icon-btn" title={t('xf.cancel')} onclick={() => transform.cancel()}><Icon name="close" size={16} /></button>
  <button class="btn primary small" title={t('xf.commit')} onclick={() => transform.commit()}>✓</button>
  <span class="hint">{t('xf.hint')}</span>
{:else}
  <span class="hint">{t('xf.moveHint')}</span>
{/if}

<style>
  .inline { display: flex; align-items: center; gap: 4px; color: var(--fg-2); font-size: var(--fs-0); }
  .inline input { width: 64px; height: 24px; text-align: center; }
  .refgrid { display: grid; grid-template-columns: repeat(3, 8px); gap: 2px; margin-inline-end: 4px; direction: ltr; }
  .dot { width: 8px; height: 8px; padding: 0; border: 1px solid var(--fg-2); border-radius: 1px; background: none; }
  .dot.on { background: var(--accent); border-color: var(--accent); }
  .small { height: 24px; padding: 0 10px; }
  .hint { color: var(--fg-3); font-size: var(--fs-0); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
</style>
