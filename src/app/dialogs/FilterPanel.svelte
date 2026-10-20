<script lang="ts">
  // A filter's settings in a floating panel (the canvas stays pannable and zoomable). The preview
  // runs on the canvas at the zoom shown, tile by tile, through a live session on the layer.
  import { onDestroy, onMount } from 'svelte';
  import { app } from '../state.svelte';
  import { t, type Key } from '../i18n/index.svelte';
  import { engine, findLayer } from '../../engine/bridge';
  import { display } from '../../engine/display';
  import { defaultFilter, newSeed, type Filter, type FilterKind } from '../../engine/filter';
  import { message } from '../actions';
  import Icon from '../icons/Icon.svelte';
  import Slider from '../panels/adjust/Slider.svelte';

  // the last settings of each filter, for the next time (with a fresh noise pattern)
  const last = ((globalThis as { __mosamemFilters?: Partial<Record<FilterKind, Filter>> }).__mosamemFilters ??= {});

  const kind = app.filterKind;
  const start = last[kind] ? { ...last[kind]! } : defaultFilter(kind);
  if (start.kind === 'addNoise') start.seed = newSeed();
  let f = $state<Filter>(start);
  let preview = $state(true);
  let working = $state(false);
  let closed = false;

  const doc = app.active;
  const layer = doc ? findLayer(doc.info.layers, doc.info.active) : null;
  const target: 'pixels' | 'mask' = app.maskTarget && layer?.mask ? 'mask' : 'pixels';
  let session: number | null = null;
  // a whole-document session: the engine composites everything with the preview (used when the
  // layer can't be split out, e.g. under an adjustment layer)
  let full = false;
  let timer = 0;

  onMount(async () => {
    if (!doc || !layer || target !== 'pixels' || layer.kind !== 'pixel') return;
    session = await engine.liveBegin(doc.info.id, layer.id).catch(() => null);
    if (session == null) {
      session = await engine.liveBeginFull(doc.info.id, layer.id).catch(() => null);
      full = session != null;
    }
    if (session == null || closed) return;
    if (full) display.compositor?.setLive({ session, blend: 'normal', alpha: 0, visible: false });
    else display.compositor?.setLive({ session, blend: layer.blend, alpha: layer.opacity * layer.fill, visible: layer.visible });
    schedule();
  });

  function schedule() {
    clearTimeout(timer);
    timer = window.setTimeout(async () => {
      if (session == null || !doc || closed) return;
      await engine.liveFilter(doc.info.id, session, preview ? $state.snapshot(f) : null).catch(() => false);
      if (!closed) display.compositor?.refreshPart(full ? 1 : 2);
    }, 40);
  }

  $effect(() => {
    void JSON.stringify(f);
    void preview;
    schedule();
  });

  function endSession(commit: boolean) {
    closed = true;
    clearTimeout(timer);
    const s = session;
    session = null;
    if (s == null) return;
    if (commit) display.compositor?.endLive(() => void engine.liveEnd(s));
    else {
      display.compositor?.cancelLive();
      void engine.liveEnd(s);
    }
  }

  function cancel() {
    endSession(false);
    app.dialog = null;
  }

  async function ok() {
    if (!doc || !layer || working) return;
    working = true;
    try {
      const settings = $state.snapshot(f) as Filter;
      app.update(await engine.filterApply(doc.info.id, layer.id, settings, target, t(`filter.${kind}` as Key)));
      last[kind] = settings;
      endSession(true);
      app.dialog = null;
    } catch (e) {
      app.notify(t('err.generic', { msg: message(e) }), 'error');
    } finally {
      working = false;
    }
  }

  onDestroy(() => {
    if (!closed) endSession(false);
  });

  const set = (patch: Record<string, unknown>) => (f = { ...f, ...patch } as Filter);
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape') cancel(); if (e.key === 'Enter' && !(e.target instanceof HTMLInputElement)) ok(); }} />

<div class="panel" role="dialog" aria-label={t(`filter.${kind}` as Key)}>
  <header>
    <h2>{t(`filter.${kind}` as Key)}</h2>
    {#if working}<span class="working">{t('filter.working')}</span>{/if}
    <button class="icon-btn" onclick={cancel} aria-label="close"><Icon name="close" size={14} /></button>
  </header>
  <div class="body">
    {#if !layer || (layer.kind !== 'pixel' && target === 'pixels')}
      <p class="note">{t('filter.notPixels')}</p>
    {:else}
      {#if target === 'mask'}<p class="note">{t('filter.onMask')}</p>{/if}
      {#if f.kind === 'gaussianBlur'}
        <Slider label={t('filter.radius')} value={f.radius} min={0.1} max={250} step={0.1} decimals={1} onchange={(v) => set({ radius: v })} />
      {:else if f.kind === 'motionBlur'}
        <Slider label={t('filter.angle')} value={f.angle} min={-90} max={90} onchange={(v) => set({ angle: v })} />
        <Slider label={t('filter.distance')} value={f.distance} min={1} max={999} onchange={(v) => set({ distance: v })} />
      {:else if f.kind === 'unsharpMask'}
        <Slider label={t('filter.amount')} value={f.amount} min={1} max={500} onchange={(v) => set({ amount: v })} />
        <Slider label={t('filter.radius')} value={f.radius} min={0.1} max={250} step={0.1} decimals={1} onchange={(v) => set({ radius: v })} />
        <Slider label={t('filter.threshold')} value={f.threshold} min={0} max={255} onchange={(v) => set({ threshold: v })} />
      {:else if f.kind === 'addNoise'}
        <Slider label={t('filter.amount')} value={f.amount} min={0.1} max={400} step={0.1} decimals={1} onchange={(v) => set({ amount: v })} />
        <div class="radios" role="radiogroup" aria-label={t('filter.distribution')}>
          <label><input type="radio" checked={!f.gaussian} onchange={() => set({ gaussian: false })} />{t('filter.uniform')}</label>
          <label><input type="radio" checked={f.gaussian} onchange={() => set({ gaussian: true })} />{t('filter.gaussian')}</label>
        </div>
        <label class="check"><input type="checkbox" checked={f.monochromatic} onchange={(e) => set({ monochromatic: e.currentTarget.checked })} />{t('filter.mono')}</label>
        <button class="btn small" onclick={() => set({ seed: newSeed() })}>{t('filter.reseed')}</button>
      {:else if f.kind === 'pixelate'}
        <Slider label={t('filter.cell')} value={f.cell} min={2} max={200} onchange={(v) => set({ cell: v })} />
      {/if}
      {#if target === 'pixels'}
        <label class="check"><input type="checkbox" bind:checked={preview} />{t('filter.preview')}</label>
      {/if}
    {/if}
  </div>
  <footer>
    <button class="btn" onclick={cancel}>{t('newdoc.cancel')}</button>
    <button class="btn primary" onclick={ok} disabled={working || !layer || (layer.kind !== 'pixel' && target === 'pixels')}>{t('filter.apply')}</button>
  </footer>
</div>

<style>
  .panel { position: fixed; top: 92px; inset-inline-end: 340px; z-index: 70; width: 300px; background: var(--bg-2); border: 1px solid var(--line-strong); border-radius: 10px; box-shadow: 0 18px 48px rgba(0, 0, 0, 0.5); }
  header { display: flex; align-items: center; gap: 8px; padding: 10px 10px 8px 14px; padding-inline-start: 14px; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: var(--fs-2); font-weight: 600; flex: 1; }
  .working { color: var(--accent); font-size: var(--fs-0); }
  .body { padding: 12px 14px; display: flex; flex-direction: column; gap: 10px; }
  .note { margin: 0; color: var(--fg-3); font-size: var(--fs-0); line-height: 1.5; }
  .radios { display: flex; gap: 14px; }
  .radios label, .check { display: flex; align-items: center; gap: 6px; color: var(--fg-1); font-size: var(--fs-1); }
  .small { align-self: flex-start; height: 24px; padding: 0 10px; font-size: var(--fs-0); }
  footer { display: flex; justify-content: flex-end; gap: 8px; padding: 10px 14px; border-top: 1px solid var(--line); }
</style>
