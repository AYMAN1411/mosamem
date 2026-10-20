<script lang="ts">
  import Modal from './Modal.svelte';
  import { app } from '../state.svelte';
  import { t, fmt, type Key } from '../i18n/index.svelte';
  import { engine, type ResampleMethod } from '../../engine/bridge';
  import { message } from '../actions';

  const d = app.active!;
  const W0 = d.info.width;
  const H0 = d.info.height;
  let width = $state(W0);
  let height = $state(H0);
  let ppi = $state(d.info.ppi);
  let constrain = $state(true);
  let resample = $state(true);
  let method = $state<ResampleMethod>('bicubic');
  let busy = $state(false);
  const methods: ResampleMethod[] = ['nearest', 'bilinear', 'bicubic', 'lanczos'];

  function setW(v: number) {
    width = Math.max(1, Math.round(v));
    if (constrain) height = Math.max(1, Math.round((width * H0) / W0));
  }
  function setH(v: number) {
    height = Math.max(1, Math.round(v));
    if (constrain) width = Math.max(1, Math.round((height * W0) / H0));
  }
  // without resampling, the pixels stay and only the resolution (print size) changes
  $effect(() => {
    if (!resample) {
      width = W0;
      height = H0;
    }
  });
  const mb = (w: number, h: number) => fmt((w * h * 4) / 1024 / 1024, 1);

  async function ok() {
    busy = true;
    try {
      app.update(await engine.imageSize(d.info.id, width, height, ppi, resample ? method : null));
      app.fit();
      app.dialog = null;
    } catch (e) {
      app.notify(t('err.generic', { msg: message(e) }), 'error');
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t('size.title')} onclose={() => (app.dialog = null)} width={380}>
  <form class="grid" onsubmit={(e) => { e.preventDefault(); ok(); }}>
    <span class="cap">{t('size.current')}</span><span class="num val">{fmt(W0)} × {fmt(H0)} px · {mb(W0, H0)} MB</span>
    <label for="is-w">{t('newdoc.width')}</label>
    <span class="field-row"><input id="is-w" class="field num" type="number" min="1" max="60000" value={width} disabled={!resample} onchange={(e) => setW(Number(e.currentTarget.value))} />px <span class="pct">{fmt((width / W0) * 100, 1)}%</span></span>
    <label for="is-h">{t('newdoc.height')}</label>
    <span class="field-row"><input id="is-h" class="field num" type="number" min="1" max="60000" value={height} disabled={!resample} onchange={(e) => setH(Number(e.currentTarget.value))} />px <span class="pct">{fmt((height / H0) * 100, 1)}%</span></span>
    <label for="is-r">{t('newdoc.resolution')}</label>
    <span class="field-row"><input id="is-r" class="field num" type="number" min="1" max="10000" step="0.01" bind:value={ppi} />ppi</span>
    <span></span><label class="check"><input type="checkbox" bind:checked={constrain} />{t('size.constrain')}</label>
    <span></span><label class="check"><input type="checkbox" bind:checked={resample} />{t('size.resample')}</label>
    <label for="is-m">{t('xf.method')}</label>
    <select id="is-m" class="field" bind:value={method} disabled={!resample}>
      {#each methods as m (m)}<option value={m}>{t(`method.${m}` as Key)}</option>{/each}
    </select>
    <span class="cap">{t('size.result')}</span><span class="num val">{fmt(width)} × {fmt(height)} px · {mb(width, height)} MB</span>
  </form>
  {#snippet footer()}
    <button class="btn" onclick={() => (app.dialog = null)}>{t('newdoc.cancel')}</button>
    <button class="btn primary" disabled={busy} onclick={ok}>{t('dlg.ok')}</button>
  {/snippet}
</Modal>

<style>
  .grid { display: grid; grid-template-columns: auto 1fr; gap: 10px 12px; align-items: center; }
  label { color: var(--fg-2); font-size: var(--fs-1); }
  .field-row { display: flex; align-items: center; gap: 6px; color: var(--fg-2); }
  .field-row input { width: 100px; }
  .pct { color: var(--fg-3); font-size: var(--fs-0); }
  .check { display: flex; align-items: center; gap: 6px; color: var(--fg-1); }
  .cap { color: var(--fg-3); font-size: var(--fs-0); }
  .val { color: var(--fg-1); }
</style>
