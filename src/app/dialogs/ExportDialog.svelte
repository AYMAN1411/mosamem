<script lang="ts">
  import Modal from './Modal.svelte';
  import { app } from '../state.svelte';
  import { t } from '../i18n/index.svelte';
  import { exportAs } from '../actions';
  import type { ExportFormat } from '../../engine/bridge';

  const isMac = navigator.userAgent.includes('Mac');
  const formats: ExportFormat[] = isMac ? ['png', 'jpeg', 'webp', 'tiff', 'heic'] : ['png', 'jpeg', 'webp', 'tiff'];
  let format = $state<ExportFormat>('png');
  let quality = $state(90);
  let lossless = $state(false);
  let busy = $state(false);
  const lossy = $derived(format === 'jpeg' || format === 'heic' || (format === 'webp' && !lossless));

  async function go() {
    busy = true;
    if (await exportAs(format, quality, lossless)) app.dialog = null;
    busy = false;
  }
</script>

<Modal title={t('export.title')} onclose={() => (app.dialog = null)} width={380}>
  <div class="form">
    <label>{t('export.format')}
      <select class="field" bind:value={format}>
        {#each formats as f (f)}<option value={f}>{f.toUpperCase()}</option>{/each}
      </select>
    </label>
    {#if format === 'webp'}
      <label class="check"><input type="checkbox" bind:checked={lossless} />{t('export.lossless')}</label>
    {/if}
    {#if lossy}
      <label>{t('export.quality')}
        <span class="slider"><input type="range" min="1" max="100" bind:value={quality} /><span class="num">{quality}</span></span>
      </label>
    {/if}
  </div>
  {#snippet footer()}
    <button class="btn" onclick={() => (app.dialog = null)}>{t('newdoc.cancel')}</button>
    <button class="btn primary" disabled={busy} onclick={go}>{t('export.save')}</button>
  {/snippet}
</Modal>

<style>
  .form { display: flex; flex-direction: column; gap: 12px; }
  .form > label:not(.check) { display: flex; flex-direction: column; gap: 4px; color: var(--fg-2); font-size: var(--fs-0); }
  .slider { display: flex; align-items: center; gap: 10px; }
  .slider input { flex: 1; accent-color: var(--accent); }
  .slider .num { width: 28px; text-align: end; color: var(--fg-0); }
</style>
