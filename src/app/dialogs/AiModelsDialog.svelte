<script lang="ts">
  // The downloaded AI models: what each is for, its size and license, and Delete / Download.
  import Modal from './Modal.svelte';
  import { app } from '../state.svelte';
  import { t, type Key } from '../i18n/index.svelte';
  import { engine, type AiPack } from '../../engine/bridge';
  import { ai, formatBytes } from '../ai.svelte';
  import { message } from '../actions';

  let busy = $state(false);

  async function remove(id: AiPack) {
    busy = true;
    try {
      await engine.aiDelete(id);
    } catch (e) {
      app.notify(t('err.generic', { msg: message(e) }), 'error');
    }
    await ai.refresh();
    busy = false;
  }

  async function get(id: AiPack) {
    app.dialog = null;
    if (await ai.ensure(id)) app.notify(t('ai.installed', { name: t(`ai.pack.${id}` as Key) }), 'info');
  }
</script>

<Modal title={t('ai.models')} onclose={() => (app.dialog = null)} width={520}>
  <p class="intro">{t('ai.modelsIntro')}</p>
  <ul>
    {#each ai.packs as p (p.id)}
      <li>
        <div class="what">
          <b>{t(`ai.pack.${p.id}` as Key)}</b>
          <small dir="ltr">{p.model} · {p.license}</small>
        </div>
        <span class="size" dir="ltr">{p.installed ? formatBytes(p.disk) : formatBytes(p.size)}</span>
        {#if p.installed}
          <button class="btn" disabled={busy} onclick={() => remove(p.id)}>{t('ai.delete')}</button>
        {:else}
          <button class="btn" disabled={busy} onclick={() => get(p.id)}>{t('ai.download')}</button>
        {/if}
      </li>
    {/each}
  </ul>
  {#snippet footer()}
    <button class="btn primary" onclick={() => (app.dialog = null)}>{t('dlg.ok')}</button>
  {/snippet}
</Modal>

<style>
  .intro { margin: 0 0 10px; color: var(--fg-2); line-height: 1.6; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  li { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border: 1px solid var(--line); border-radius: var(--radius-sm); }
  .what { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .what b { font-weight: 600; color: var(--fg-1); }
  .what small { color: var(--fg-3); font-size: var(--fs-0); text-align: start; }
  .size { color: var(--fg-2); font-size: var(--fs-0); }
</style>
