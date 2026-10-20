<script lang="ts">
  // Asks before a model's first download, then shows its progress. The model is fetched once,
  // checked by SHA-256, and after that the feature works offline.
  import { onDestroy } from 'svelte';
  import Modal from './Modal.svelte';
  import { t, type Key } from '../i18n/index.svelte';
  import { engine } from '../../engine/bridge';
  import { ai, formatBytes } from '../ai.svelte';
  import { message } from '../actions';

  const pack = ai.asking!;
  const info = ai.packs.find((p) => p.id === pack);
  let phase = $state<'ask' | 'downloading' | 'error'>('ask');
  let done = $state(0);
  let total = $state(info?.size ?? 0);
  let error = $state('');

  const pct = $derived(total ? Math.min(100, Math.round((done / total) * 100)) : 0);

  async function download() {
    phase = 'downloading';
    done = 0;
    try {
      await engine.aiDownload(pack, (d, tt) => {
        done = d;
        total = tt;
      });
      ai.done(true);
    } catch (e) {
      const m = message(e);
      if (m === 'cancelled') return ai.done(false);
      error = m.startsWith('network') ? t('ai.err.network') : m.startsWith('checksum') ? t('ai.err.checksum') : m.startsWith('disk') ? t('ai.err.disk', { msg: m.slice(6) }) : m;
      phase = 'error';
    }
  }

  function cancel() {
    if (phase === 'downloading') void engine.aiCancel(pack);
    else ai.done(false);
  }

  onDestroy(() => {
    if (phase === 'downloading') void engine.aiCancel(pack);
  });
</script>

<Modal title={t(`ai.pack.${pack}` as Key)} onclose={cancel} width={440}>
  <div class="body">
    <p>{t(`ai.pack.${pack}.why` as Key)}</p>
    {#if info}
      <dl>
        <dt>{t('ai.model')}</dt><dd dir="ltr">{info.model}</dd>
        <dt>{t('ai.size')}</dt><dd dir="ltr">{formatBytes(info.size)}</dd>
        <dt>{t('ai.license')}</dt><dd dir="ltr">{info.license}</dd>
        <dt>{t('ai.source')}</dt><dd dir="ltr" class="src">{info.source}</dd>
      </dl>
    {/if}
    <p class="note">{t('ai.private')}</p>
    {#if phase === 'downloading'}
      <div class="bar" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}><span style:width={`${pct}%`}></span></div>
      <p class="progress" dir="ltr">{formatBytes(done)} / {formatBytes(total)} · {pct}%</p>
    {:else if phase === 'error'}
      <p class="error">{error}</p>
    {/if}
  </div>
  {#snippet footer()}
    <button class="btn" onclick={cancel}>{t('newdoc.cancel')}</button>
    {#if phase !== 'downloading'}
      <button class="btn primary" onclick={download}>{phase === 'error' ? t('ai.retry') : t('ai.download')}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .body { display: flex; flex-direction: column; gap: 10px; }
  p { margin: 0; color: var(--fg-1); line-height: 1.6; }
  dl { display: grid; grid-template-columns: auto 1fr; gap: 4px 12px; margin: 0; font-size: var(--fs-0); }
  dt { color: var(--fg-3); }
  dd { margin: 0; color: var(--fg-1); text-align: start; }
  .src { overflow-wrap: anywhere; color: var(--fg-2); }
  .note { color: var(--fg-2); font-size: var(--fs-0); }
  .bar { height: 6px; border-radius: 3px; background: var(--bg-4); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--accent); transition: width 0.2s; }
  .progress { color: var(--fg-2); font-size: var(--fs-0); text-align: center; }
  .error { color: var(--danger, #e5484d); }
</style>
