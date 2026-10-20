<script lang="ts">
  import Modal from './Modal.svelte';
  import { app } from '../state.svelte';
  import { t, fmt, type Key } from '../i18n/index.svelte';
  import { engine } from '../../engine/bridge';
  import { message } from '../actions';

  const d = app.active!;
  const W0 = d.info.width;
  const H0 = d.info.height;
  let w = $state(W0);
  let h = $state(H0);
  let relative = $state(false);
  // where the old canvas sits in the new one, column and row 0–2
  let anchor = $state(4);
  let extension = $state<'transparent' | 'white' | 'black' | 'background' | 'foreground'>('transparent');
  let busy = $state(false);
  const exts = ['transparent', 'white', 'black', 'background', 'foreground'] as const;

  const width = $derived(relative ? W0 + w : w);
  const height = $derived(relative ? H0 + h : h);
  $effect(() => {
    // switching between absolute and relative keeps the result
    if (relative) {
      w = 0;
      h = 0;
    } else {
      w = W0;
      h = H0;
    }
  });

  function hex(c: string): [number, number, number, number] {
    return [parseInt(c.slice(1, 3), 16), parseInt(c.slice(3, 5), 16), parseInt(c.slice(5, 7), 16), 255];
  }

  async function ok() {
    if (width < 1 || height < 1) return;
    busy = true;
    const fill: [number, number, number, number] | null =
      extension === 'transparent' ? null : extension === 'white' ? [255, 255, 255, 255] : extension === 'black' ? [0, 0, 0, 255] : hex(extension === 'background' ? app.bg : app.fg);
    try {
      app.update(await engine.canvasSize(d.info.id, width, height, [(anchor % 3) / 2, Math.floor(anchor / 3) / 2], fill));
      app.fit();
      app.dialog = null;
    } catch (e) {
      app.notify(t('err.generic', { msg: message(e) }), 'error');
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t('canvas.title')} onclose={() => (app.dialog = null)} width={400}>
  <form class="grid" onsubmit={(e) => { e.preventDefault(); ok(); }}>
    <span class="cap">{t('size.current')}</span><span class="num val">{fmt(W0)} × {fmt(H0)} px</span>
    <label for="cs-w">{t('newdoc.width')}</label>
    <span class="field-row"><input id="cs-w" class="field num" type="number" min={relative ? 1 - W0 : 1} max="60000" bind:value={w} />px</span>
    <label for="cs-h">{t('newdoc.height')}</label>
    <span class="field-row"><input id="cs-h" class="field num" type="number" min={relative ? 1 - H0 : 1} max="60000" bind:value={h} />px</span>
    <span></span><label class="check"><input type="checkbox" bind:checked={relative} />{t('canvas.relative')}</label>
    <span class="lbl">{t('canvas.anchor')}</span>
    <div class="anchor" dir="ltr" role="radiogroup" aria-label={t('canvas.anchor')}>
      {#each [0, 1, 2, 3, 4, 5, 6, 7, 8] as i (i)}
        <button type="button" class="cell" class:on={anchor === i} onclick={() => (anchor = i)} aria-label={`${i}`}></button>
      {/each}
    </div>
    <label for="cs-x">{t('canvas.extension')}</label>
    <select id="cs-x" class="field" bind:value={extension}>
      {#each exts as x (x)}<option value={x}>{t(`canvas.${x}` as Key)}</option>{/each}
    </select>
    <span class="cap">{t('size.result')}</span><span class="num val">{fmt(width)} × {fmt(height)} px</span>
  </form>
  {#snippet footer()}
    <button class="btn" onclick={() => (app.dialog = null)}>{t('newdoc.cancel')}</button>
    <button class="btn primary" disabled={busy || width < 1 || height < 1} onclick={ok}>{t('dlg.ok')}</button>
  {/snippet}
</Modal>

<style>
  .grid { display: grid; grid-template-columns: auto 1fr; gap: 10px 12px; align-items: center; }
  label, .lbl { color: var(--fg-2); font-size: var(--fs-1); }
  .field-row { display: flex; align-items: center; gap: 6px; color: var(--fg-2); }
  .field-row input { width: 100px; }
  .check { display: flex; align-items: center; gap: 6px; color: var(--fg-1); }
  .anchor { display: grid; grid-template-columns: repeat(3, 26px); gap: 3px; }
  .cell { width: 26px; height: 26px; padding: 0; border: 1px solid var(--line-strong); border-radius: 4px; background: var(--bg-1); }
  .cell.on { background: var(--accent); border-color: var(--accent); }
  .cap { color: var(--fg-3); font-size: var(--fs-0); }
  .val { color: var(--fg-1); }
</style>
