<script lang="ts">
  import { app } from '../state.svelte';
  import { t } from '../i18n/index.svelte';
  import Icon from '../icons/Icon.svelte';

  const hex = (v: string) => (/^#?[0-9a-f]{6}$/i.test(v) ? (v.startsWith('#') ? v : `#${v}`).toLowerCase() : null);
</script>

<div class="color">
  <div class="swatches">
    <label class="sw fg" style:background={app.fg} title={t('tool.foreground')}>
      <input type="color" bind:value={app.fg} />
    </label>
    <label class="sw bg" style:background={app.bg} title={t('tool.background')}>
      <input type="color" bind:value={app.bg} />
    </label>
  </div>
  <div class="fields">
    <label>{t('tool.foreground')}
      <input class="field num" value={app.fg} onchange={(e) => { const v = hex(e.currentTarget.value); if (v) app.fg = v; }} />
    </label>
    <label>{t('tool.background')}
      <input class="field num" value={app.bg} onchange={(e) => { const v = hex(e.currentTarget.value); if (v) app.bg = v; }} />
    </label>
    <div class="row">
      <button class="icon-btn" title={t('tool.swapColors')} onclick={() => ([app.fg, app.bg] = [app.bg, app.fg])}><Icon name="swap" /></button>
      <button class="icon-btn" title={t('tool.defaultColors')} onclick={() => { app.fg = '#000000'; app.bg = '#ffffff'; }}><Icon name="defaultColors" /></button>
    </div>
  </div>
</div>

<style>
  .color { display: flex; gap: 14px; padding: 12px; }
  .swatches { position: relative; width: 64px; height: 64px; flex: none; }
  .sw { position: absolute; width: 40px; height: 40px; border-radius: 4px; border: 2px solid var(--bg-0); box-shadow: 0 0 0 1px var(--line-strong); }
  .sw input { opacity: 0; width: 100%; height: 100%; }
  .fg { top: 0; inset-inline-start: 0; z-index: 1; }
  .bg { bottom: 0; inset-inline-end: 0; }
  .fields { display: flex; flex-direction: column; gap: 6px; flex: 1; min-width: 0; }
  .fields label { display: flex; flex-direction: column; gap: 3px; color: var(--fg-2); font-size: var(--fs-0); }
  .row { display: flex; gap: 4px; }
</style>
