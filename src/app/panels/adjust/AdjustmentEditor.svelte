<script lang="ts">
  // Settings of the active adjustment layer. Edits show at once (transient engine updates) and
  // become one history step when a control is released.
  import { engine, type LayerInfo } from '../../../engine/bridge';
  import { defaultAdjustment, type Adjustment, type Histogram } from '../../../engine/adjust';
  import { adjust, layer as layerAct } from '../../actions';
  import { liveAdjust, liveAdjustCommit } from '../../live';
  import { t, type Key } from '../../i18n/index.svelte';
  import Icon from '../../icons/Icon.svelte';
  import { adjustmentIcon } from './icons';
  import Slider from './Slider.svelte';
  import LevelsEditor from './LevelsEditor.svelte';
  import CurvesEditor from './CurvesEditor.svelte';
  import HistogramView from './Histogram.svelte';
  import HueSatEditor from './HueSatEditor.svelte';
  import ColorBalanceEditor from './ColorBalanceEditor.svelte';
  import BlackWhiteEditor from './BlackWhiteEditor.svelte';
  import GradientMapEditor from './GradientMapEditor.svelte';

  let { doc, layer, historyIndex }: { doc: number; layer: LayerInfo; historyIndex: number } = $props();

  const clone = (a: Adjustment): Adjustment => JSON.parse(JSON.stringify(a));
  let draft = $state<Adjustment | null>(null);
  let editing = false;
  let draftFor = -1;

  // follow the engine (undo, another layer) unless a control is being dragged
  $effect(() => {
    const a = layer.adjustment;
    const id = layer.id;
    if (!a) return;
    if (id !== draftFor || !editing) {
      draftFor = id;
      draft = clone(a);
    }
  });

  // while dragging, the GPU previews the change (or, where it can't, the engine does without a
  // history step); releasing records one step
  function change(next: Adjustment, live: boolean) {
    draft = next;
    const snapshot = clone(next);
    const id = layer.id;
    if (live) {
      editing = true;
      void liveAdjust(id, snapshot).then((ok) => ok || adjust.preview(id, snapshot));
    } else {
      void liveAdjustCommit(id, snapshot).finally(() => (editing = false));
    }
  }

  // the histogram of what the layer adjusts (for levels, curves and threshold)
  let hist = $state<Histogram | null>(null);
  const needsHist = $derived(draft?.kind === 'levels' || draft?.kind === 'curves' || draft?.kind === 'threshold');
  $effect(() => {
    if (!needsHist) return;
    const key = `${doc}:${layer.id}:${historyIndex}`;
    void key;
    let cancelled = false;
    engine.histogram(doc, layer.id).then((h) => !cancelled && (hist = h)).catch(() => {});
    return () => (cancelled = true);
  });
</script>

{#if draft}
<section class="adj">
  <header>
    <span class="kind-icon"><Icon name={adjustmentIcon(draft.kind)} size={16} /></span>
    <h3>{t(`adj.${draft.kind}` as Key)}</h3>
    <button class="icon-btn small" title={t('adj.clip')} aria-pressed={layer.clipped} onclick={() => layerAct.set(layer.id, { clipped: !layer.clipped })}>↳</button>
    <button class="btn small-btn" onclick={() => draft && change(defaultAdjustment(draft.kind), false)}>{t('adj.reset')}</button>
  </header>

  {#if draft.kind === 'levels'}
    <LevelsEditor value={draft} {hist} onchange={change} />
  {:else if draft.kind === 'curves'}
    <CurvesEditor value={draft} {hist} onchange={change} />
  {:else if draft.kind === 'hueSaturation'}
    <HueSatEditor value={draft} onchange={change} />
  {:else if draft.kind === 'brightnessContrast'}
    {@const a = draft}
    <Slider label={t('adj.brightness')} value={a.brightness} min={-150} max={150} onchange={(v, live) => change({ ...a, brightness: v }, live)} />
    <Slider label={t('adj.contrast')} value={a.contrast} min={-50} max={100} onchange={(v, live) => change({ ...a, contrast: v }, live)} />
    <label class="check"><input type="checkbox" checked={a.legacy} onchange={(e) => change({ ...a, legacy: e.currentTarget.checked }, false)} />{t('adj.legacy')}</label>
  {:else if draft.kind === 'exposure'}
    {@const a = draft}
    <Slider label={t('adj.exposureAmount')} value={a.exposure} min={-20} max={20} step={0.01} decimals={2} onchange={(v, live) => change({ ...a, exposure: v }, live)} />
    <Slider label={t('adj.offset')} value={a.offset} min={-0.5} max={0.5} step={0.0001} decimals={4} onchange={(v, live) => change({ ...a, offset: v }, live)} />
    <Slider label={t('adj.gammaCorrection')} value={a.gamma} min={0.01} max={9.99} step={0.01} decimals={2} onchange={(v, live) => change({ ...a, gamma: v }, live)} />
  {:else if draft.kind === 'vibrance'}
    {@const a = draft}
    <Slider label={t('adj.vibranceAmount')} value={a.vibrance} min={-100} max={100} track="linear-gradient(90deg, #8c8c8c, #c0503c)" onchange={(v, live) => change({ ...a, vibrance: v }, live)} />
    <Slider label={t('adj.saturation')} value={a.saturation} min={-100} max={100} track="linear-gradient(90deg, #8c8c8c, #e0302a)" onchange={(v, live) => change({ ...a, saturation: v }, live)} />
  {:else if draft.kind === 'colorBalance'}
    <ColorBalanceEditor value={draft} onchange={change} />
  {:else if draft.kind === 'blackWhite'}
    <BlackWhiteEditor value={draft} onchange={change} />
  {:else if draft.kind === 'gradientMap'}
    <GradientMapEditor value={draft} onchange={change} />
  {:else if draft.kind === 'threshold'}
    {@const a = draft}
    <HistogramView data={hist?.luminosity ?? null} marks={[a.level]} />
    <Slider label={t('adj.level')} value={a.level} min={1} max={255} track="linear-gradient(90deg, #000, #fff)" onchange={(v, live) => change({ ...a, level: v }, live)} />
  {:else if draft.kind === 'invert'}
    <p class="none">{t('adj.noSettings')}</p>
  {/if}
</section>
{/if}

<style>
  .adj { padding: 10px 12px; display: flex; flex-direction: column; gap: 10px; }
  header { display: flex; align-items: center; gap: 6px; }
  .kind-icon { display: grid; color: var(--fg-1); }
  h3 { flex: 1; margin: 0; font-size: var(--fs-1); font-weight: 600; }
  .small { width: 24px; height: 24px; }
  .small-btn { height: 22px; padding: 0 8px; font-size: var(--fs-0); }
  .check { display: flex; align-items: center; gap: 6px; color: var(--fg-1); font-size: var(--fs-1); }
  .none { margin: 0; color: var(--fg-3); font-size: var(--fs-1); }
</style>
