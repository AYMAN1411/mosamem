<script lang="ts">
  // Layer Style: the effects of the active layer, previewed on the canvas while they change.
  // OK keeps them as one history step; Cancel puts the layer back as it was.
  import { onDestroy } from 'svelte';
  import { app } from '../state.svelte';
  import { t, type Key } from '../i18n/index.svelte';
  import { engine, findLayer } from '../../engine/bridge';
  import { BLEND_MODES, type BlendMode } from '../../engine/blend';
  import { EFFECTS, defaultEffect, type EffectKind, type LayerStyle } from '../../engine/style';
  import { message } from '../actions';
  import Icon from '../icons/Icon.svelte';
  import Slider from '../panels/adjust/Slider.svelte';

  const doc = app.active;
  const layer = doc ? findLayer(doc.info.layers, doc.info.active) : null;
  const original: LayerStyle | null = layer?.style ? JSON.parse(JSON.stringify(layer.style)) : null;
  let style = $state<LayerStyle>(original ? JSON.parse(JSON.stringify(original)) : {});
  let current = $state<EffectKind>(app.styleEffect ?? EFFECTS.find((k) => style[k]?.enabled) ?? 'dropShadow');
  let done = false;

  // at most one preview in flight; the latest wins
  let busy = false;
  let queued: LayerStyle | null = null;
  async function preview(s: LayerStyle) {
    if (!doc || !layer) return;
    queued = s;
    if (busy) return;
    busy = true;
    while (queued) {
      const q = queued;
      queued = null;
      try {
        app.update(await engine.layerStyleSet(doc.info.id, layer.id, q, true));
      } catch (e) {
        app.notify(t('err.generic', { msg: message(e) }), 'error');
        break;
      }
    }
    busy = false;
  }

  $effect(() => {
    void preview($state.snapshot(style) as LayerStyle);
  });

  function toggle(k: EffectKind, on: boolean) {
    const e = style[k] ?? defaultEffect(k);
    style = { ...style, [k]: { ...e, enabled: on } };
    current = k;
  }
  function set(patch: Record<string, unknown>) {
    const e = style[current] ?? defaultEffect(current);
    style = { ...style, [current]: { ...e, ...patch, enabled: true } };
  }
  const eff = $derived((style[current] ?? defaultEffect(current)) as unknown as Record<string, unknown> & { blend: BlendMode; color?: number[]; opacity: number });

  const hex = (c: number[]) => `#${c.map((v) => v.toString(16).padStart(2, '0')).join('')}`;
  const rgb = (h: string): [number, number, number] => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];

  async function finish(keep: boolean) {
    if (!doc || !layer || done) return;
    done = true;
    while (busy) await new Promise((r) => setTimeout(r, 5));
    try {
      if (keep) app.update(await engine.layerStyleSet(doc.info.id, layer.id, $state.snapshot(style) as LayerStyle, false));
      else app.update(await engine.layerStyleSet(doc.info.id, layer.id, original, true));
    } catch (e) {
      app.notify(t('err.generic', { msg: message(e) }), 'error');
    }
    app.dialog = null;
  }

  onDestroy(() => {
    if (!done) void finish(false);
  });
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape') finish(false); if (e.key === 'Enter' && !(e.target instanceof HTMLInputElement)) finish(true); }} />

<div class="panel" role="dialog" aria-label={t('style.title')}>
  <header>
    <h2>{t('style.title')}</h2>
    <button class="icon-btn" onclick={() => finish(false)} aria-label="close"><Icon name="close" size={14} /></button>
  </header>
  {#if !layer || (layer.kind !== 'pixel' && layer.kind !== 'text')}
    <p class="note">{t('style.notHere')}</p>
  {:else}
    <div class="body">
      <ul class="list">
        {#each EFFECTS as k (k)}
          <li class:sel={k === current}>
            <input type="checkbox" checked={!!style[k]?.enabled} onchange={(e) => toggle(k, e.currentTarget.checked)} aria-label={t(`style.${k}` as Key)} />
            <button onclick={() => (current = k)}>{t(`style.${k}` as Key)}</button>
          </li>
        {/each}
      </ul>
      <div class="settings">
        <label class="row">{t('style.blend')}
          <select class="field" value={eff.blend} onchange={(e) => set({ blend: e.currentTarget.value })}>
            {#each BLEND_MODES as m (m)}<option value={m} selected={m === eff.blend}>{t(`blend.${m}` as Key)}</option>{/each}
          </select>
        </label>
        {#if eff.color}
          <label class="row">{t('style.color')}<input type="color" value={hex(eff.color)} onchange={(e) => set({ color: rgb(e.currentTarget.value) })} /></label>
        {/if}
        <Slider label={t('style.opacity')} value={Math.round(eff.opacity * 100)} min={0} max={100} onchange={(v) => set({ opacity: v / 100 })} />
        {#if current === 'dropShadow' || current === 'innerShadow'}
          <Slider label={t('style.angle')} value={eff.angle as number} min={-180} max={180} onchange={(v) => set({ angle: v })} />
          <Slider label={t('style.distance')} value={eff.distance as number} min={0} max={300} onchange={(v) => set({ distance: v })} />
          <Slider label={t(current === 'dropShadow' ? 'style.spread' : 'style.choke')} value={eff.spread as number} min={0} max={100} onchange={(v) => set({ spread: v })} />
          <Slider label={t('style.size')} value={eff.size as number} min={0} max={250} onchange={(v) => set({ size: v })} />
          {#if current === 'dropShadow'}
            <label class="check"><input type="checkbox" checked={eff.knockout as boolean} onchange={(e) => set({ knockout: e.currentTarget.checked })} />{t('style.knockout')}</label>
          {/if}
        {:else if current === 'outerGlow'}
          <label class="row">{t('style.technique')}
            <select class="field" value={eff.technique} onchange={(e) => set({ technique: e.currentTarget.value })}>
              <option value="softer" selected={eff.technique === 'softer'}>{t('style.softer')}</option>
              <option value="precise" selected={eff.technique === 'precise'}>{t('style.precise')}</option>
            </select>
          </label>
          <Slider label={t('style.spread')} value={eff.spread as number} min={0} max={100} onchange={(v) => set({ spread: v })} />
          <Slider label={t('style.size')} value={eff.size as number} min={0} max={250} onchange={(v) => set({ size: v })} />
        {:else if current === 'stroke'}
          <Slider label={t('style.size')} value={eff.size as number} min={1} max={250} onchange={(v) => set({ size: v })} />
          <label class="row">{t('style.position')}
            <select class="field" value={eff.position} onchange={(e) => set({ position: e.currentTarget.value })}>
              {#each ['outside', 'inside', 'center'] as p (p)}<option value={p} selected={eff.position === p}>{t(`style.pos.${p}` as Key)}</option>{/each}
            </select>
          </label>
        {:else if current === 'gradientOverlay'}
          {@const stops = eff.stops as { pos: number; color: [number, number, number] }[]}
          <div class="row">{t('style.gradient')}
            <span class="grad" dir="ltr">
              <input type="color" value={hex(stops[0].color)} onchange={(e) => set({ stops: [{ ...stops[0], color: rgb(e.currentTarget.value) }, stops[stops.length - 1]] })} />
              <span class="bar" style:background={`linear-gradient(90deg, ${hex(stops[0].color)}, ${hex(stops[stops.length - 1].color)})`}></span>
              <input type="color" value={hex(stops[stops.length - 1].color)} onchange={(e) => set({ stops: [stops[0], { ...stops[stops.length - 1], color: rgb(e.currentTarget.value) }] })} />
            </span>
          </div>
          <label class="row">{t('style.gradStyle')}
            <select class="field" value={eff.style} onchange={(e) => set({ style: e.currentTarget.value })}>
              {#each ['linear', 'radial', 'reflected'] as p (p)}<option value={p} selected={eff.style === p}>{t(`grad.${p}` as Key)}</option>{/each}
            </select>
          </label>
          <Slider label={t('style.angle')} value={eff.angle as number} min={-180} max={180} onchange={(v) => set({ angle: v })} />
          <Slider label={t('style.scale')} value={eff.scale as number} min={10} max={150} onchange={(v) => set({ scale: v })} />
          <label class="check"><input type="checkbox" checked={eff.reverse as boolean} onchange={(e) => set({ reverse: e.currentTarget.checked })} />{t('opt.reverse')}</label>
        {/if}
      </div>
    </div>
  {/if}
  <footer>
    <button class="btn" onclick={() => finish(false)}>{t('newdoc.cancel')}</button>
    <button class="btn primary" onclick={() => finish(true)}>{t('dlg.ok')}</button>
  </footer>
</div>

<style>
  .panel { position: fixed; top: 92px; inset-inline-end: 340px; z-index: 70; width: 470px; background: var(--bg-2); border: 1px solid var(--line-strong); border-radius: 10px; box-shadow: 0 18px 48px rgba(0, 0, 0, 0.5); }
  header { display: flex; align-items: center; gap: 8px; padding: 10px 10px 8px 14px; padding-inline-start: 14px; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: var(--fs-2); font-weight: 600; flex: 1; }
  .body { display: grid; grid-template-columns: 150px minmax(0, 1fr); min-height: 300px; }
  .list { margin: 0; padding: 8px 6px; list-style: none; border-inline-end: 1px solid var(--line); }
  .list li { display: flex; align-items: center; gap: 6px; padding: 2px 6px; border-radius: var(--radius-sm); }
  .list li.sel { background: var(--bg-4); }
  .list button { flex: 1; padding: 4px 0; border: 0; background: none; color: var(--fg-1); text-align: start; font-size: var(--fs-1); }
  .settings { padding: 12px 14px; display: flex; flex-direction: column; gap: 10px; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 8px; color: var(--fg-1); font-size: var(--fs-1); }
  .row select { max-width: 170px; }
  .check { display: flex; align-items: center; gap: 6px; color: var(--fg-1); font-size: var(--fs-1); }
  input[type='color'] { width: 34px; height: 22px; padding: 0; border: 1px solid var(--line-strong); border-radius: var(--radius-sm); background: none; }
  .grad { display: flex; align-items: center; gap: 6px; }
  .bar { width: 90px; height: 14px; border-radius: 3px; box-shadow: 0 0 0 1px var(--line-strong); }
  .note { margin: 0; padding: 16px; color: var(--fg-3); }
  footer { display: flex; justify-content: flex-end; gap: 8px; padding: 10px 14px; border-top: 1px solid var(--line); }
</style>
