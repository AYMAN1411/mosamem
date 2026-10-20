<script lang="ts">
  // File › Export: what to export (the picture, the active layer or the selection), at what
  // size, in which format, with the file's size worked out as the settings change.
  import Modal from './Modal.svelte';
  import { app } from '../state.svelte';
  import { t } from '../i18n/index.svelte';
  import { exportAs, message } from '../actions';
  import { engine, findLayer, type ExportFormat, type ExportSource } from '../../engine/bridge';
  import { formatBytes } from '../ai.svelte';

  const isMac = navigator.userAgent.includes('Mac');
  const formats: ExportFormat[] = isMac ? ['png', 'jpeg', 'webp', 'tiff', 'heic'] : ['png', 'jpeg', 'webp', 'tiff'];
  const doc = app.active!;
  const layer = findLayer(doc.info.layers, doc.info.active);

  let format = $state<ExportFormat>('png');
  let quality = $state(90);
  let lossless = $state(false);
  let what = $state<'document' | 'layer' | 'selection'>('document');
  /** The source's own size, from the engine. */
  let natural = $state<[number, number]>([doc.info.width, doc.info.height]);
  let width = $state(doc.info.width);
  let height = $state(doc.info.height);
  let estimate = $state<{ bytes: number; size: [number, number] } | null>(null);
  let estimating = $state(false);
  let problem = $state('');
  let busy = $state(false);

  const lossy = $derived(format === 'jpeg' || format === 'heic' || (format === 'webp' && !lossless));
  const source = $derived<ExportSource>(what === 'layer' && layer ? { kind: 'layer', id: layer.id } : what === 'selection' ? { kind: 'selection' } : { kind: 'document' });
  const resized = $derived(width !== natural[0] || height !== natural[1]);
  const scale = $derived(Math.round((width / natural[0]) * 1000) / 10);

  // long side of the web presets, px
  const PRESETS = [3840, 1920, 1280, 1080, 800];

  function setWidth(v: number) {
    width = Math.max(1, Math.round(v));
    height = Math.max(1, Math.round((width * natural[1]) / natural[0]));
  }
  function setHeight(v: number) {
    height = Math.max(1, Math.round(v));
    width = Math.max(1, Math.round((height * natural[0]) / natural[1]));
  }
  function setScale(pct: number) {
    setWidth((natural[0] * pct) / 100);
  }
  function preset(v: string) {
    if (v === '100') setScale(100);
    else if (v === '50') setScale(50);
    else if (v === '200') setScale(200);
    else {
      const long = Number(v);
      if (natural[0] >= natural[1]) setWidth(long);
      else setHeight(long);
    }
  }

  // a new source has its own size: start from it again
  let lastSource = '';
  $effect(() => {
    const key = JSON.stringify(source);
    if (key === lastSource) return;
    lastSource = key;
    engine
      .exportNatural(doc.info.id, source)
      .then((n) => {
        natural = n;
        width = n[0];
        height = n[1];
        problem = '';
      })
      .catch((e) => (problem = message(e)));
  });

  // the file's size, worked out shortly after the settings stop changing
  let ticket = 0;
  $effect(() => {
    const spec = { doc: doc.info.id, format, quality, lossless, source, size: resized ? ([width, height] as [number, number]) : null };
    const mine = ++ticket;
    estimating = true;
    const id = setTimeout(async () => {
      try {
        const e = await engine.exportEstimate(spec);
        if (mine === ticket) {
          estimate = e;
          problem = '';
        }
      } catch (e) {
        if (mine === ticket) problem = message(e);
      } finally {
        if (mine === ticket) estimating = false;
      }
    }, 350);
    return () => clearTimeout(id);
  });

  const problemText = $derived(problem === 'no-selection' ? t('export.noSelection') : problem === 'empty-layer' ? t('ai.err.emptyLayer') : problem === 'too-large' ? t('export.tooLarge') : problem);

  async function go() {
    busy = true;
    const suffix = what === 'layer' && layer ? `-${layer.name}` : what === 'selection' ? `-${t('export.selectionSuffix')}` : '';
    if (await exportAs(format, quality, lossless, source, resized ? [width, height] : null, suffix)) app.dialog = null;
    busy = false;
  }
</script>

<Modal title={t('export.title')} onclose={() => (app.dialog = null)} width={420}>
  <div class="form">
    <label>{t('export.what')}
      <select class="field" bind:value={what}>
        <option value="document">{t('export.whole')}</option>
        <option value="layer" disabled={!layer}>{t('export.layer', { name: layer?.name ?? '' })}</option>
        <option value="selection" disabled={!doc.info.hasSelection}>{t('export.selection')}</option>
      </select>
    </label>
    <div class="row">
      <label>{t('export.width')}<input class="field num" type="number" min="1" value={width} onchange={(e) => setWidth(+e.currentTarget.value)} /></label>
      <span class="x">×</span>
      <label>{t('export.height')}<input class="field num" type="number" min="1" value={height} onchange={(e) => setHeight(+e.currentTarget.value)} /></label>
      <label class="pct">%<input class="field num" type="number" min="1" max="1000" step="1" value={scale} onchange={(e) => setScale(+e.currentTarget.value)} /></label>
    </div>
    <label>{t('export.presets')}
      <select class="field" value="" onchange={(e) => { preset(e.currentTarget.value); e.currentTarget.value = ''; }}>
        <option value="" disabled>{t('export.choose')}</option>
        <option value="100">{t('export.original')}</option>
        <option value="50">50%</option>
        <option value="200">200%</option>
        {#each PRESETS as p (p)}<option value={String(p)}>{t('export.longSide', { px: p })}</option>{/each}
      </select>
    </label>
    <label>{t('export.format')}
      <select class="field" bind:value={format}>
        {#each formats as f (f)}<option value={f}>{f.toUpperCase()}</option>{/each}
      </select>
    </label>
    {#if format === 'webp'}
      <label class="check"><input type="checkbox" bind:checked={lossless} />{t('export.lossless')}</label>
    {/if}
    {#if lossy}
      <label>{t('export.quality')}
        <span class="slider"><input type="range" min="1" max="100" bind:value={quality} /><span class="num">{quality}</span></span>
      </label>
    {/if}
    <p class="estimate" class:stale={estimating} aria-live="polite">
      {#if problem}<span class="error">{problemText}</span>
      {:else if estimate}<span dir="ltr">≈ {formatBytes(estimate.bytes)} · {estimate.size[0]} × {estimate.size[1]} px</span>
      {:else}{t('export.estimating')}{/if}
    </p>
  </div>
  {#snippet footer()}
    <button class="btn" onclick={() => (app.dialog = null)}>{t('newdoc.cancel')}</button>
    <button class="btn primary" disabled={busy || !!problem} onclick={go}>{t('export.save')}</button>
  {/snippet}
</Modal>

<style>
  .form { display: flex; flex-direction: column; gap: 12px; }
  .form label:not(.check) { display: flex; flex-direction: column; gap: 4px; color: var(--fg-2); font-size: var(--fs-0); }
  .row { display: flex; align-items: flex-end; gap: 8px; }
  .row label { flex: 1; }
  .row .pct { flex: 0 0 72px; }
  .x { color: var(--fg-3); padding-bottom: 6px; }
  .slider { display: flex; align-items: center; gap: 10px; }
  .slider input { flex: 1; accent-color: var(--accent); }
  .slider .num { width: 28px; text-align: end; color: var(--fg-0); }
  .check { display: flex; align-items: center; gap: 6px; color: var(--fg-1); }
  .estimate { margin: 0; padding: 8px 10px; border-radius: var(--radius-sm); background: var(--bg-3); color: var(--fg-1); font-size: var(--fs-0); text-align: center; transition: opacity 0.15s; }
  .estimate.stale { opacity: 0.55; }
  .error { color: var(--danger, #e5484d); }
</style>
