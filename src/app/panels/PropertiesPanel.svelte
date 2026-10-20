<script lang="ts">
  import { app } from '../state.svelte';
  import { t, fmt } from '../i18n/index.svelte';
  import { findLayer } from '../../engine/bridge';
  import { mask } from '../actions';
  import Scrub from './Scrub.svelte';

  const active = $derived(app.active ? findLayer(app.active.info.layers, app.active.info.active) : null);

  // feather in px shown through the percent-style scrub as 0–1 of 250 px
  const FEATHER_MAX = 250;
</script>

{#if app.active}
  {@const info = app.active.info}
  {#if active?.mask && app.maskTarget}
    {@const m = active.mask}
    <section class="mask">
      <h3>{t('mask.title')}</h3>
      <div class="row"><Scrub label={t('mask.density')} value={m.density} onchange={(v, live) => mask.set({ density: v }, live)} /></div>
      <div class="row">
        <Scrub label={t('mask.feather')} value={m.feather / FEATHER_MAX} onchange={(v, live) => mask.set({ feather: v * FEATHER_MAX }, live)} />
        <span class="num px">{fmt(m.feather, 1)} px</span>
      </div>
      <div class="buttons">
        <button class="btn" onclick={mask.invert}>{t('mask.invert')}</button>
        <button class="btn" onclick={() => mask.toSelection(active.id)}>{t('mask.load')}</button>
        <button class="btn" onclick={() => mask.toggle()}>{m.enabled ? t('mask.disable') : t('mask.enable')}</button>
        <button class="btn" onclick={mask.apply} disabled={active.kind !== 'pixel'}>{t('mask.apply')}</button>
        <button class="btn" onclick={mask.delete}>{t('mask.delete')}</button>
      </div>
    </section>
  {:else}
    <dl class="props">
      <dt>{t('newdoc.width')}</dt><dd class="num">{fmt(info.width)} px</dd>
      <dt>{t('newdoc.height')}</dt><dd class="num">{fmt(info.height)} px</dd>
      <dt>{t('newdoc.resolution')}</dt><dd class="num">{fmt(info.ppi)} ppi</dd>
    </dl>
  {/if}
{:else}
  <div class="empty">{t('panel.empty')}</div>
{/if}

<style>
  .props { display: grid; grid-template-columns: auto 1fr; gap: 6px 12px; margin: 0; padding: 12px; }
  dt { color: var(--fg-2); }
  dd { margin: 0; text-align: end; }
  .mask { padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; }
  h3 { margin: 0; font-size: var(--fs-1); font-weight: 600; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .px { color: var(--fg-3); font-size: var(--fs-0); }
  .buttons { display: flex; flex-wrap: wrap; gap: 6px; }
  .buttons .btn { height: 24px; padding: 0 8px; font-size: var(--fs-0); }
  .empty { padding: 16px; color: var(--fg-3); }
</style>
