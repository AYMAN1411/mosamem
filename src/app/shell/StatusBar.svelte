<script lang="ts">
  import { app } from '../state.svelte';
  import { t, fmt } from '../i18n/index.svelte';

  const pct = $derived(app.active ? fmt(app.active.view.zoom * 100, app.active.view.zoom < 0.1 ? 2 : 1) : '');

  function setZoom(v: string) {
    const n = parseFloat(v.replace('%', '').replace('٪', ''));
    if (n > 0) app.zoomTo(n / 100);
  }
</script>

<footer class="status">
  {#if app.active}
    <label class="zoom">
      <span class="sr">{t('status.zoom')}</span>
      <input class="field num" value={`${pct}%`} onchange={(e) => setZoom(e.currentTarget.value)} />
    </label>
    <span class="num">{t('status.size', { w: app.active.info.width, h: app.active.info.height })}</span>
    <span class="num">{t('status.ppi', { ppi: app.active.info.ppi })}</span>
    {#if app.active.view.rotation !== 0}<span class="num">{fmt(app.active.view.rotation)}°</span>{/if}
  {/if}
</footer>

<style>
  .status { display: flex; align-items: center; gap: 16px; height: var(--statusbar-h); padding-inline: 8px; background: var(--bg-1); border-top: 1px solid var(--line); color: var(--fg-2); font-size: var(--fs-0); }
  .zoom input { width: 70px; height: 18px; font-size: var(--fs-0); }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); }
</style>
