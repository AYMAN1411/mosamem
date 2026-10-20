<script lang="ts">
  import Modal from './Modal.svelte';
  import { app } from '../state.svelte';
  import { t, type Key } from '../i18n/index.svelte';
  import { select } from '../actions';

  const titles: Record<string, Key> = { feather: 'select.feather', expand: 'select.expand', contract: 'select.contract', border: 'select.border', smooth: 'select.smooth' };
  const defaults: Record<string, number> = { feather: 5, expand: 5, contract: 5, border: 10, smooth: 4 };
  const op = app.modifyOp;
  let amount = $state(defaults[op]);
  let busy = $state(false);

  async function ok() {
    busy = true;
    await select.modify({ op, amount: Math.max(op === 'feather' ? 0.1 : 1, amount) } as never);
    busy = false;
    app.dialog = null;
  }
</script>

<Modal title={t(titles[op]).replace('…', '')} onclose={() => (app.dialog = null)} width={320}>
  <form class="row" onsubmit={(e) => { e.preventDefault(); ok(); }}>
    <label>{t('modify.amount')}<input class="field num" type="number" min="0.1" max="1000" step={op === 'feather' ? 0.1 : 1} bind:value={amount} /></label>
    <span class="unit">{t('modify.px')}</span>
  </form>
  {#snippet footer()}
    <button class="btn" onclick={() => (app.dialog = null)}>{t('newdoc.cancel')}</button>
    <button class="btn primary" disabled={busy} onclick={ok}>{t('modify.ok')}</button>
  {/snippet}
</Modal>

<style>
  .row { display: flex; align-items: flex-end; gap: 8px; }
  label { display: flex; flex-direction: column; gap: 4px; color: var(--fg-2); font-size: var(--fs-0); flex: 1; }
  .unit { color: var(--fg-2); padding-bottom: 6px; }
</style>
