<script lang="ts">
  // Asks, one document at a time, whether to save changes before closing.
  import Modal from './Modal.svelte';
  import { app } from '../state.svelte';
  import { t } from '../i18n/index.svelte';
  import { pendingClose, saveDocument, forceClose } from '../actions';

  let busy = $state(false);
  let current = $state(app.docs.find((d) => d.info.id === pendingClose.docs[0]) ?? null);

  async function next() {
    pendingClose.docs.shift();
    const d = app.docs.find((x) => x.info.id === pendingClose.docs[0]);
    if (d) {
      current = d;
      app.activeId = d.info.id;
    } else {
      app.dialog = null;
      const then = pendingClose.then;
      pendingClose.then = null;
      then?.();
    }
  }

  async function save() {
    if (!current) return;
    busy = true;
    const ok = await saveDocument(current);
    busy = false;
    if (!ok) return;
    await forceClose(current.info.id);
    await next();
  }

  async function discard() {
    if (!current) return;
    await forceClose(current.info.id);
    await next();
  }

  function cancel() {
    pendingClose.docs = [];
    pendingClose.then = null;
    app.dialog = null;
  }
</script>

{#if current}
  <Modal title={t('save.unsavedTitle')} onclose={cancel} width={440}>
    <p class="msg">{t('save.unsavedOne', { name: current.info.name })}</p>
    {#snippet footer()}
      <button class="btn" onclick={discard} disabled={busy}>{t('save.dontSave')}</button>
      <span class="spacer"></span>
      <button class="btn" onclick={cancel} disabled={busy}>{t('save.cancel')}</button>
      <button class="btn primary" onclick={save} disabled={busy}>{t('save.save')}</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .msg { margin: 0; font-size: var(--fs-2); line-height: 1.6; }
  .spacer { flex: 1; }
</style>
