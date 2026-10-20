<script lang="ts">
  import { crop, RATIOS } from '../canvas/crop.svelte';
  import { t, fmt, type Key } from '../i18n/index.svelte';
  import Icon from '../icons/Icon.svelte';

  const r = $derived(crop.rect);
  const label = (id: string) => (id === 'free' || id === 'original' ? t(`crop.${id}` as Key) : id);
  const num = (e: Event & { currentTarget: HTMLInputElement }) => parseFloat(e.currentTarget.value.replace(/[٫,]/, '.'));
</script>

<label class="inline">{t('crop.ratio')}
  <select class="field" value={crop.ratio} onchange={(e) => crop.setRatio(e.currentTarget.value)}>
    {#each RATIOS as x (x.id)}<option value={x.id} selected={crop.ratio === x.id}>{label(x.id)}</option>{/each}
  </select>
</label>
{#if r}
  <label class="inline">W<input class="field num" value={fmt(r.w)} onchange={(e) => crop.setSize(num(e), r.h)} /></label>
  <label class="inline">H<input class="field num" value={fmt(r.h)} onchange={(e) => crop.setSize(r.w, num(e))} /></label>
  <label class="inline"><Icon name="rotateView" size={15} /><input class="field num" value={`${fmt(r.angle, 1)}°`} onchange={(e) => crop.setAngle(num(e) || 0)} /></label>
{/if}
<button class="btn small" aria-pressed={crop.straightening} class:on={crop.straightening} title={t('crop.straightenHint')} onclick={() => (crop.straightening = !crop.straightening)}>{t('crop.straighten')}</button>
<label class="check"><input type="checkbox" bind:checked={crop.deleteCropped} />{t('crop.delete')}</label>
<button class="icon-btn" title={t('crop.cancel')} onclick={() => crop.reset()}><Icon name="close" size={16} /></button>
<button class="btn primary small" title={t('crop.commit')} onclick={() => crop.commit()}>✓</button>

<style>
  .inline { display: flex; align-items: center; gap: 4px; color: var(--fg-2); font-size: var(--fs-0); }
  .inline input { width: 64px; height: 24px; text-align: center; }
  .check { display: flex; align-items: center; gap: 5px; color: var(--fg-1); font-size: var(--fs-0); }
  .small { height: 24px; padding: 0 10px; }
  .on { border-color: var(--accent); color: var(--accent); }
</style>
