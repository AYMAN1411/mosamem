<script lang="ts">
  import Modal from './Modal.svelte';
  import { app } from '../state.svelte';
  import { t, fmt, type Key } from '../i18n/index.svelte';
  import { engine, type Background } from '../../engine/bridge';

  interface Preset { key: Key; w: number; h: number; ppi: number; note: string }
  const presets: Preset[] = [
    { key: 'preset.instagramPost', w: 1080, h: 1350, ppi: 72, note: '4:5' },
    { key: 'preset.instagramSquare', w: 1080, h: 1080, ppi: 72, note: '1:1' },
    { key: 'preset.instagramStory', w: 1080, h: 1920, ppi: 72, note: '9:16' },
    { key: 'preset.youtubeThumb', w: 1280, h: 720, ppi: 72, note: '16:9' },
    { key: 'preset.fullHd', w: 1920, h: 1080, ppi: 72, note: '16:9' },
    { key: 'preset.a4', w: 2480, h: 3508, ppi: 300, note: '210 × 297 mm' },
  ];
  type Unit = 'px' | 'mm' | 'cm' | 'in';
  const perInch: Record<Exclude<Unit, 'px'>, number> = { mm: 25.4, cm: 2.54, in: 1 };

  let selected = $state<number>(0);
  let name = $state(t('newdoc.defaultName', { n: app.untitledCount + 1 }));
  let width = $state(presets[0].w); // always px
  let height = $state(presets[0].h);
  let ppi = $state(presets[0].ppi);
  let unit = $state<Unit>('px');
  let bg = $state<'white' | 'black' | 'transparent' | 'color'>('white');
  let bgColor = $state('#3fc4aa');
  let busy = $state(false);
  let error = $state('');

  function pick(i: number) {
    selected = i;
    if (i < presets.length) {
      ({ w: width, h: height, ppi } = presets[i]);
      unit = presets[i].ppi >= 300 ? 'mm' : 'px';
    }
  }

  const toUnit = (px: number) => (unit === 'px' ? px : (px / ppi) * perInch[unit]);
  const fromUnit = (v: number) => Math.round(unit === 'px' ? v : (v / perInch[unit]) * ppi);
  const shown = (px: number) => fmt(toUnit(px), unit === 'px' ? 0 : 2);

  function setW(v: string) { width = Math.max(1, fromUnit(parseFloat(v) || 0)); selected = presets.length; }
  function setH(v: string) { height = Math.max(1, fromUnit(parseFloat(v) || 0)); selected = presets.length; }

  async function create() {
    if (width < 1 || height < 1 || width > 30000 || height > 30000) {
      error = '1 – 30000 px';
      return;
    }
    busy = true;
    const hex = bgColor.replace('#', '');
    const background: Background =
      bg === 'color' ? { kind: 'color', rgba: [parseInt(hex.slice(0, 2), 16), parseInt(hex.slice(2, 4), 16), parseInt(hex.slice(4, 6), 16), 255] } : { kind: bg };
    try {
      const info = await engine.newDocument({ name, width, height, ppi, background, backgroundLayerName: t('layer.background') });
      app.untitledCount++;
      app.addDocument(info);
      app.dialog = null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t('newdoc.title')} onclose={() => (app.dialog = null)} width={680}>
  <div class="layout">
    <div class="presets" role="listbox" aria-label={t('newdoc.presets')}>
      {#each presets as p, i (p.key)}
        <button class="preset" role="option" aria-selected={selected === i} onclick={() => pick(i)}>
          <span class="shape" style:aspect-ratio={`${p.w} / ${p.h}`}></span>
          <span class="label">{t(p.key)}</span>
          <span class="dims num">{p.w} × {p.h}</span>
          <span class="dims">{p.note}</span>
        </button>
      {/each}
      <button class="preset" role="option" aria-selected={selected === presets.length} onclick={() => pick(presets.length)}>
        <span class="shape custom"></span>
        <span class="label">{t('preset.custom')}</span>
      </button>
    </div>

    <form class="form" onsubmit={(e) => { e.preventDefault(); create(); }}>
      <label>{t('newdoc.name')}<input class="field" bind:value={name} /></label>
      <div class="row">
        <label>{t('newdoc.width')}<input class="field num" inputmode="decimal" value={shown(width)} onchange={(e) => setW(e.currentTarget.value)} /></label>
        <label>{t('newdoc.height')}<input class="field num" inputmode="decimal" value={shown(height)} onchange={(e) => setH(e.currentTarget.value)} /></label>
        <label class="unit">&nbsp;
          <select class="field" bind:value={unit}>
            {#each ['px', 'mm', 'cm', 'in'] as const as u (u)}<option value={u}>{t(`newdoc.unit.${u}`)}</option>{/each}
          </select>
        </label>
      </div>
      <div class="row">
        <label>{t('newdoc.resolution')}<span class="with-unit"><input class="field num" type="number" min="1" max="5000" bind:value={ppi} /><span>{t('newdoc.ppi')}</span></span></label>
        <label>{t('newdoc.orientation')}
          <span class="seg">
            <button type="button" class="btn" aria-pressed={height >= width} onclick={() => { if (width > height) [width, height] = [height, width]; }}>{t('newdoc.portrait')}</button>
            <button type="button" class="btn" aria-pressed={width > height} onclick={() => { if (height > width) [width, height] = [height, width]; }}>{t('newdoc.landscape')}</button>
          </span>
        </label>
      </div>
      <label>{t('newdoc.background')}
        <span class="with-unit">
          <select class="field" bind:value={bg}>
            <option value="white">{t('newdoc.bg.white')}</option>
            <option value="black">{t('newdoc.bg.black')}</option>
            <option value="transparent">{t('newdoc.bg.transparent')}</option>
            <option value="color">{t('newdoc.bg.color')}</option>
          </select>
          {#if bg === 'color'}<input type="color" bind:value={bgColor} />{/if}
        </span>
      </label>
      <p class="summary num">{fmt(width)} × {fmt(height)} px · {fmt((width * height) / 1e6, 1)} MP</p>
      {#if error}<p class="error">{error}</p>{/if}
      <button type="submit" hidden aria-label={t('newdoc.create')}></button>
    </form>
  </div>

  {#snippet footer()}
    <button class="btn" onclick={() => (app.dialog = null)}>{t('newdoc.cancel')}</button>
    <button class="btn primary" disabled={busy} onclick={create}>{t('newdoc.create')}</button>
  {/snippet}
</Modal>

<style>
  .layout { display: grid; grid-template-columns: 1fr 260px; gap: 18px; }
  .presets { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; align-content: start; }
  .preset { display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 10px 6px; border: 1px solid var(--line); border-radius: 8px; background: var(--bg-1); cursor: default; }
  .preset:hover { border-color: var(--line-strong); }
  .preset[aria-selected='true'] { border-color: var(--accent); background: var(--accent-soft); }
  .shape { height: 46px; max-width: 80px; border: 1.5px solid var(--fg-2); border-radius: 3px; margin-bottom: 4px; }
  .shape.custom { width: 40px; border-style: dashed; }
  .label { font-weight: 600; font-size: var(--fs-1); text-align: center; }
  .dims { color: var(--fg-3); font-size: var(--fs-0); }
  .form { display: flex; flex-direction: column; gap: 12px; }
  .form label { display: flex; flex-direction: column; gap: 4px; color: var(--fg-2); font-size: var(--fs-0); min-width: 0; }
  .row { display: grid; grid-template-columns: 1fr 1fr auto; gap: 8px; }
  .row:has(.seg) { grid-template-columns: 1fr 1.4fr; }
  .unit select { width: 72px; }
  .with-unit { display: flex; align-items: center; gap: 6px; color: var(--fg-2); }
  .with-unit input[type='number'] { width: 80px; }
  .seg { display: flex; gap: 4px; }
  .seg .btn { flex: 1; height: 26px; padding: 0 6px; font-size: var(--fs-0); }
  .seg .btn[aria-pressed='true'] { border-color: var(--accent); color: var(--accent); }
  .summary { margin: 0; color: var(--fg-3); }
  .error { margin: 0; color: var(--danger); }
</style>
