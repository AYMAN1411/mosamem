<script lang="ts">
  import { app } from '../state.svelte';
  import { TOOLS } from '../tools';
  import { t, i18n, fmt } from '../i18n/index.svelte';
  import Icon from '../icons/Icon.svelte';
  import * as act from '../actions';

  const tool = $derived(TOOLS[app.tool]);
</script>

<header class="options">
  <div class="tool"><Icon name={tool.icon} size={18} /><span>{t(tool.title)}</span></div>
  <div class="sep"></div>

  {#if app.tool === 'zoom'}
    <button class="icon-btn" title={t('opt.zoomIn')} onclick={act.view.zoomIn}><Icon name="zoom" /></button>
    <button class="icon-btn" title={t('opt.zoomOut')} onclick={act.view.zoomOut}><Icon name="zoomOut" /></button>
    <button class="btn" onclick={act.view.actual}>{t('opt.actual')}</button>
    <button class="btn" onclick={act.view.fit}>{t('opt.fit')}</button>
  {:else if app.tool === 'hand'}
    <button class="btn" onclick={act.view.actual}>{t('opt.actual')}</button>
    <button class="btn" onclick={act.view.fit}>{t('opt.fit')}</button>
  {:else if app.tool === 'rotateView'}
    <label class="inline">{t('opt.angle')}
      <input
        class="field num angle"
        type="number"
        step="1"
        value={fmt(app.active?.view.rotation ?? 0)}
        onchange={(e) => app.rotateTo(Number(e.currentTarget.value) || 0)}
      />°
    </label>
    <button class="btn" onclick={act.view.resetRotation}>{t('opt.reset')}</button>
  {:else if tool.milestone > 1}
    <span class="note">{t('tool.notYet')}</span>
  {/if}

  <span class="spacer"></span>
  <button class="btn lang" title={t('lang.switch')} onclick={() => i18n.toggle()}>
    <Icon name="globe" size={16} />{t('lang.other')}
  </button>
</header>

<style>
  .options { display: flex; align-items: center; gap: 6px; height: var(--optionsbar-h); padding-inline: 10px; background: var(--bg-1); border-bottom: 1px solid var(--line); }
  .tool { display: flex; align-items: center; gap: 8px; min-width: 120px; font-weight: 600; }
  .spacer { flex: 1; }
  .note { color: var(--fg-3); }
  .inline { display: flex; align-items: center; gap: 6px; color: var(--fg-2); }
  .angle { width: 64px; }
  .lang { height: 26px; }
</style>
