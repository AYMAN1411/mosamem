<script lang="ts">
  // The Pen and Direct Selection tools' path commands: path ↔ selection, fill and stroke.
  import { app } from '../state.svelte';
  import { pen } from '../canvas/pen.svelte';
  import { activeShape } from '../canvas/shapes.svelte';
  import { t } from '../i18n/index.svelte';

  const work = $derived(app.active?.workPath ?? null);
  const hasPath = $derived((void app.active?.info.version, !!work || !!activeShape()));
  const hasSelection = $derived(!!app.active?.info.hasSelection);
</script>

<button class="btn small" disabled={!hasPath} title={t('path.toSelection.tip')} onclick={() => pen.toSelection()}>{t('path.toSelection')}</button>
<button class="btn small" disabled={!hasSelection} title={t('path.fromSelection.tip')} onclick={() => pen.fromSelection()}>{t('path.fromSelection')}</button>
<button class="btn small" disabled={!work} title={t('path.fill.tip')} onclick={() => pen.paintPath('fill')}>{t('path.fill')}</button>
<button class="btn small" disabled={!work} title={t('path.stroke.tip')} onclick={() => pen.paintPath('stroke')}>{t('path.stroke')}</button>
<button class="btn small" disabled={!work} onclick={() => pen.deleteWorkPath()}>{t('path.delete')}</button>

<style>
  .small { height: 24px; padding: 0 8px; font-size: var(--fs-0); }
</style>
