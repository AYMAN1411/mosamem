<script lang="ts">
  // Refine Edges: a floating panel (not modal) so the canvas stays pannable and zoomable while the
  // refined selection is previewed live.
  import { onDestroy } from 'svelte';
  import { app } from '../state.svelte';
  import { t, type Key } from '../i18n/index.svelte';
  import { engine, type RefineOutput, type RefineParams } from '../../engine/bridge';
  import { display } from '../../engine/display';
  import type { RefineView } from '../../engine/compositor';
  import { message } from '../actions';
  import Icon from '../icons/Icon.svelte';

  const views: RefineView[] = ['overlay', 'black', 'white', 'mask', 'layers', 'ants'];
  const outputs: RefineOutput[] = ['selection', 'layerMask', 'newLayer', 'newLayerWithMask'];
  let view = $state<RefineView>('overlay');
  let p = $state<RefineParams>({ radius: 8, smooth: 0, feather: 0, contrast: 0, shift: 0 });
  let decontaminate = $state(false);
  let output = $state<RefineOutput>('selection');
  let working = $state(false);
  let timer = 0;
  let seq = 0;

  $effect(() => {
    if (display.compositor) {
      display.compositor.refineView = view;
      display.compositor.requestFrame();
    }
  });

  // live preview, debounced
  $effect(() => {
    const params = { ...p };
    clearTimeout(timer);
    timer = window.setTimeout(async () => {
      const d = app.active;
      if (!d) return;
      const mine = ++seq;
      working = true;
      try {
        const info = await engine.refinePreview(d.info.id, params);
        if (mine === seq) app.update(info);
      } catch (e) {
        app.notify(t('err.generic', { msg: message(e) }), 'error');
      } finally {
        if (mine === seq) working = false;
      }
    }, 180);
  });

  function close() {
    if (display.compositor) {
      display.compositor.refineView = 'ants';
      display.compositor.requestFrame();
    }
    app.dialog = null;
  }

  async function cancel() {
    const d = app.active;
    if (d) app.update(await engine.refineCancel(d.info.id));
    close();
  }

  async function ok() {
    const d = app.active;
    if (!d) return;
    working = true;
    try {
      app.update(await engine.refineApply(d.info.id, { ...p }, output, decontaminate, t('refine.layerName')));
      close();
    } catch (e) {
      app.notify(t('err.generic', { msg: message(e) }), 'error');
    } finally {
      working = false;
    }
  }

  // however the panel goes away, the canvas returns to normal marching ants and a pending preview
  // is dropped
  onDestroy(() => {
    clearTimeout(timer);
    if (display.compositor) {
      display.compositor.refineView = 'ants';
      display.compositor.requestFrame();
    }
    const d = app.active;
    if (d?.info.refining) void engine.refineCancel(d.info.id).then((i) => app.update(i));
  });

  const sliders: { key: keyof RefineParams; label: Key; min: number; max: number; step: number; unit: string }[] = [
    { key: 'radius', label: 'refine.radius', min: 0, max: 250, step: 1, unit: 'px' },
    { key: 'smooth', label: 'refine.smooth', min: 0, max: 100, step: 1, unit: '' },
    { key: 'feather', label: 'refine.feather', min: 0, max: 250, step: 0.5, unit: 'px' },
    { key: 'contrast', label: 'refine.contrast', min: 0, max: 100, step: 1, unit: '%' },
    { key: 'shift', label: 'refine.shift', min: -100, max: 100, step: 1, unit: '%' },
  ];
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape') cancel(); if (e.key === 'Enter' && !(e.target instanceof HTMLInputElement)) ok(); }} />

<div class="panel" role="dialog" aria-label={t('refine.title')}>
  <header>
    <h2>{t('refine.title')}</h2>
    {#if working}<span class="working">{t('refine.working')}</span>{/if}
    <button class="icon-btn" onclick={cancel} aria-label="close"><Icon name="close" size={14} /></button>
  </header>
  <div class="body">
    <label class="line">{t('refine.view')}
      <select class="field" bind:value={view}>
        {#each views as v (v)}<option value={v}>{t(`refine.view.${v}` as Key)}</option>{/each}
      </select>
    </label>
    <h3>{t('refine.edge')}</h3>
    {#each sliders.slice(0, 1) as s (s.key)}
      <label class="slider">{t(s.label)}
        <input type="range" min={s.min} max={s.max} step={s.step} bind:value={p[s.key]} />
        <span class="num">{p[s.key]}{s.unit}</span>
      </label>
    {/each}
    <h3>{t('refine.adjust')}</h3>
    {#each sliders.slice(1) as s (s.key)}
      <label class="slider">{t(s.label)}
        <input type="range" min={s.min} max={s.max} step={s.step} bind:value={p[s.key]} />
        <span class="num">{p[s.key]}{s.unit}</span>
      </label>
    {/each}
    <h3>{t('refine.output')}</h3>
    <label class="check"><input type="checkbox" bind:checked={decontaminate} disabled={!output.startsWith('new')} />{t('refine.decontaminate')}</label>
    <select class="field" bind:value={output}>
      {#each outputs as o (o)}<option value={o}>{t(`refine.out.${o}` as Key)}</option>{/each}
    </select>
  </div>
  <footer>
    <button class="btn" onclick={cancel}>{t('newdoc.cancel')}</button>
    <button class="btn primary" onclick={ok} disabled={working}>{t('refine.ok')}</button>
  </footer>
</div>

<style>
  .panel { position: fixed; top: 92px; inset-inline-end: 340px; z-index: 70; width: 312px; background: var(--bg-2); border: 1px solid var(--line-strong); border-radius: 10px; box-shadow: 0 18px 48px rgba(0, 0, 0, 0.5); }
  header { display: flex; align-items: center; gap: 8px; padding: 10px 10px 8px 14px; padding-inline-start: 14px; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: var(--fs-2); font-weight: 600; flex: 1; }
  .working { color: var(--accent); font-size: var(--fs-0); }
  .body { padding: 10px 14px; display: flex; flex-direction: column; gap: 8px; }
  h3 { margin: 6px 0 0; font-size: var(--fs-0); color: var(--fg-3); font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; }
  .line { display: flex; align-items: center; justify-content: space-between; gap: 8px; color: var(--fg-2); }
  .slider { display: grid; grid-template-columns: 92px 1fr 52px; align-items: center; gap: 8px; color: var(--fg-1); }
  .slider input { accent-color: var(--accent); }
  .slider .num { text-align: end; color: var(--fg-2); }
  footer { display: flex; justify-content: flex-end; gap: 8px; padding: 10px 14px; border-top: 1px solid var(--line); }
</style>
