<script lang="ts">
  // Edit › Smart Fill: fills the selection from what's around it, with the learned model (MI-GAN,
  // downloaded on first use) or the classic patch search (no download).
  import Modal from './Modal.svelte';
  import { app } from '../state.svelte';
  import { t } from '../i18n/index.svelte';
  import { ai, aiActions, formatBytes } from '../ai.svelte';

  const KEY = 'lawha.smartFill';
  let saved: { method?: 'ai' | 'classic'; newLayer?: boolean } = {};
  try {
    saved = JSON.parse(localStorage.getItem(KEY) ?? '{}');
  } catch {
    saved = {};
  }
  let method = $state<'ai' | 'classic'>(saved.method ?? 'ai');
  let newLayer = $state(saved.newLayer ?? true);
  const fill = $derived(ai.packs.find((p) => p.id === 'fill'));

  function ok() {
    try {
      localStorage.setItem(KEY, JSON.stringify({ method, newLayer }));
    } catch {
      // not remembered; fine
    }
    app.dialog = null;
    void aiActions.smartFill(method, newLayer);
  }
</script>

<Modal title={t('ai.smartFill')} onclose={() => (app.dialog = null)} width={420}>
  <form class="body" onsubmit={(e) => { e.preventDefault(); ok(); }}>
    <p>{t('ai.smartFillHint')}</p>
    <label class="choice">
      <input type="radio" name="method" value="ai" bind:group={method} />
      <span><b>{t('ai.methodAi')}</b><small>{t('ai.methodAiHint')}{#if fill && !fill.installed} · {t('ai.needsDownload', { size: formatBytes(fill.size) })}{/if}</small></span>
    </label>
    <label class="choice">
      <input type="radio" name="method" value="classic" bind:group={method} />
      <span><b>{t('ai.methodClassic')}</b><small>{t('ai.methodClassicHint')}</small></span>
    </label>
    <label class="check"><input type="checkbox" bind:checked={newLayer} />{t('ai.onNewLayer')}</label>
  </form>
  {#snippet footer()}
    <button class="btn" onclick={() => (app.dialog = null)}>{t('newdoc.cancel')}</button>
    <button class="btn primary" onclick={ok}>{t('ai.fill')}</button>
  {/snippet}
</Modal>

<style>
  .body { display: flex; flex-direction: column; gap: 10px; }
  p { margin: 0; color: var(--fg-2); line-height: 1.6; }
  input { accent-color: var(--accent); }
  .choice { display: flex; gap: 8px; align-items: flex-start; padding: 8px; border: 1px solid var(--line); border-radius: var(--radius-sm); cursor: pointer; }
  .choice:has(input:checked) { border-color: var(--accent); }
  .choice span { display: flex; flex-direction: column; gap: 2px; }
  .choice b { font-weight: 600; color: var(--fg-1); }
  .choice small { color: var(--fg-3); font-size: var(--fs-0); line-height: 1.5; }
  .check { display: flex; align-items: center; gap: 6px; color: var(--fg-1); }
</style>
