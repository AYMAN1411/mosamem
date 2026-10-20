<script lang="ts">
  import Modal from './Modal.svelte';
  import { app } from '../state.svelte';
  import { t, i18n, fmt } from '../i18n/index.svelte';
  import { engine, type RecoveredFile } from '../../engine/bridge';
  import { message } from '../actions';

  let { files }: { files: RecoveredFile[] } = $props();
  // the list is fixed when the dialog opens
  // svelte-ignore state_referenced_locally
  let left = $state([...files]);

  async function restore(f: RecoveredFile) {
    try {
      const info = await engine.recoveryOpen(f.file);
      app.addDocument(info);
    } catch (e) {
      app.notify(t('err.open', { msg: message(e) }), 'error');
    }
    left = left.filter((x) => x !== f);
    if (!left.length) app.dialog = null;
  }

  async function discard(f: RecoveredFile) {
    await engine.recoveryDiscard(f.file);
    left = left.filter((x) => x !== f);
    if (!left.length) app.dialog = null;
  }

  const when = (s: number) => new Date(s * 1000).toLocaleString(i18n.locale === 'ar' ? 'ar-EG-u-nu-latn' : 'en-GB', { dateStyle: 'medium', timeStyle: 'short' });
</script>

<Modal title={t('recovery.title')} onclose={() => (app.dialog = null)} width={520}>
  <p class="body">{t('recovery.body')}</p>
  <ul class="files">
    {#each left as f (f.file)}
      <li>
        <div class="meta">
          <strong>{f.name}</strong>
          <span class="sub"><span class="num">{fmt(f.width)} × {fmt(f.height)}</span> · {when(f.savedAt)}</span>
        </div>
        <button class="btn" onclick={() => discard(f)}>{t('recovery.discard')}</button>
        <button class="btn primary" onclick={() => restore(f)}>{t('recovery.restore')}</button>
      </li>
    {/each}
  </ul>
  {#snippet footer()}
    <button class="btn" onclick={() => (app.dialog = null)}>{t('recovery.later')}</button>
    <button class="btn primary" onclick={async () => { for (const f of [...left]) await restore(f); }}>{t('recovery.restoreAll')}</button>
  {/snippet}
</Modal>

<style>
  .body { margin: 0 0 12px; color: var(--fg-1); line-height: 1.6; }
  .files { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  li { display: flex; align-items: center; gap: 8px; padding: 10px 12px; border: 1px solid var(--line); border-radius: 8px; background: var(--bg-1); }
  .meta { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .sub { color: var(--fg-3); font-size: var(--fs-0); }
</style>
