<script lang="ts">
  // A shape layer in the Properties panel: its geometry as numbers, then its fill and stroke.
  import type { LayerInfo } from '../../engine/bridge';
  import type { Shape, StrokeAlign } from '../../engine/vector';
  import { shapeTool, optionsOf, type Dash } from '../canvas/shapes.svelte';
  import { t, fmt, type Key } from '../i18n/index.svelte';

  let { layer }: { layer: LayerInfo } = $props();

  const data = $derived(layer.shape!);
  const s = $derived(data.shape);
  const o = $derived(optionsOf(data, shapeTool.opts));
  const line = $derived(s.kind === 'line');

  type Field = { label: Key; value: number; set: (v: number) => Shape | null; unit?: string };
  const fields = $derived.by((): Field[] => {
    const sh = s;
    const box = (k: 'x' | 'y' | 'w' | 'h', label: Key): Field => ({
      label,
      value: (sh as unknown as Record<string, number>)[k],
      set: (v) => (k === 'w' || k === 'h') && v <= 0 ? null : ({ ...sh, [k]: v } as Shape),
      unit: 'px',
    });
    switch (sh.kind) {
      case 'rect':
        return [box('x', 'shape.x'), box('y', 'shape.y'), box('w', 'shape.w'), box('h', 'shape.h'), { label: 'shape.radius', value: sh.radius, set: (v) => (v < 0 ? null : { ...sh, radius: v }), unit: 'px' }];
      case 'ellipse':
        return [box('x', 'shape.x'), box('y', 'shape.y'), box('w', 'shape.w'), box('h', 'shape.h')];
      case 'polygon':
      case 'star': {
        const common: Field[] = [
          { label: 'shape.cx', value: sh.cx, set: (v) => ({ ...sh, cx: v }), unit: 'px' },
          { label: 'shape.cy', value: sh.cy, set: (v) => ({ ...sh, cy: v }), unit: 'px' },
          { label: 'shape.r', value: sh.r, set: (v) => (v <= 0 ? null : { ...sh, r: v }), unit: 'px' },
          { label: 'shape.angle', value: sh.angle, set: (v) => ({ ...sh, angle: v }), unit: '°' },
        ];
        return sh.kind === 'polygon'
          ? [...common, { label: 'shape.sides', value: sh.sides, set: (v) => (v < 3 ? null : { ...sh, sides: Math.round(v) }) }]
          : [
              ...common,
              { label: 'shape.points', value: sh.points, set: (v) => (v < 3 ? null : { ...sh, points: Math.round(v) }) },
              { label: 'shape.inner', value: sh.inner * 100, set: (v) => (v <= 0 || v > 100 ? null : { ...sh, inner: v / 100 }), unit: '%' },
            ];
      }
      case 'line':
        return [
          { label: 'shape.x1', value: sh.a[0], set: (v) => ({ ...sh, a: [v, sh.a[1]] }), unit: 'px' },
          { label: 'shape.y1', value: sh.a[1], set: (v) => ({ ...sh, a: [sh.a[0], v] }), unit: 'px' },
          { label: 'shape.x2', value: sh.b[0], set: (v) => ({ ...sh, b: [v, sh.b[1]] }), unit: 'px' },
          { label: 'shape.y2', value: sh.b[1], set: (v) => ({ ...sh, b: [sh.b[0], v] }), unit: 'px' },
        ];
      case 'free':
        return [];
    }
  });

  function setField(f: Field, e: Event & { currentTarget: HTMLInputElement }) {
    const v = parseFloat(e.currentTarget.value.replace(/[٫,]/, '.'));
    const next = isNaN(v) ? null : f.set(v);
    if (next) void shapeTool.update(layer, { shape: next, paint: data.paint }, false);
    else e.currentTarget.value = fmt(f.value, 1);
  }
  const num = (e: Event & { currentTarget: HTMLInputElement }) => parseFloat(e.currentTarget.value.replace(/[٫,]/, '.'));
</script>

<section class="shape">
  <h3>{t(`shape.${s.kind}` as Key)}</h3>
  {#if fields.length}
    <div class="grid">
      {#each fields as f (f.label)}
        <label>
          <span>{t(f.label)}</span>
          <input class="field num" value={fmt(f.value, 1)} onfocus={(e) => e.currentTarget.select()} onchange={(e) => setField(f, e)} />
          <span class="unit">{f.unit ?? ''}</span>
        </label>
      {/each}
    </div>
  {/if}
  {#if s.kind === 'line'}
    <div class="row">
      <label class="check"><input type="checkbox" checked={s.arrowStart} onchange={(e) => shapeTool.set({ arrowStart: e.currentTarget.checked })} />{t('shape.arrowStart')}</label>
      <label class="check"><input type="checkbox" checked={s.arrowEnd} onchange={(e) => shapeTool.set({ arrowEnd: e.currentTarget.checked })} />{t('shape.arrowEnd')}</label>
    </div>
  {/if}

  <h4>{t('shape.appearance')}</h4>
  {#if !line}
    <div class="row">
      <label class="check"><input type="checkbox" checked={o.fillOn} onchange={(e) => shapeTool.set({ fillOn: e.currentTarget.checked })} />{t('shape.fill')}</label>
      {#if data.paint.fill?.kind === 'linear'}
        <span class="note">{t('shape.gradientFill')}</span>
      {:else}
        <input class="swatch" type="color" value={o.fill} disabled={!o.fillOn} onchange={(e) => shapeTool.set({ fill: e.currentTarget.value, fillOn: true })} />
      {/if}
    </div>
    <div class="row">
      <label class="check"><input type="checkbox" checked={o.strokeOn} onchange={(e) => shapeTool.set({ strokeOn: e.currentTarget.checked })} />{t('shape.stroke')}</label>
      <input class="swatch" type="color" value={o.stroke} disabled={!o.strokeOn} onchange={(e) => shapeTool.set({ stroke: e.currentTarget.value, strokeOn: true })} />
    </div>
  {:else}
    <div class="row">
      <span>{t('shape.color')}</span>
      <input class="swatch" type="color" value={o.stroke} onchange={(e) => shapeTool.set({ stroke: e.currentTarget.value })} />
    </div>
  {/if}
  <div class="row">
    <span>{line ? t('shape.weight') : t('shape.width')}</span>
    <span><input class="field num" value={fmt(o.width, 1)} onchange={(e) => num(e) >= 0 && shapeTool.set({ width: num(e) })} /> <span class="unit">px</span></span>
  </div>
  {#if !line}
    <div class="row">
      <span>{t('shape.align')}</span>
      <select class="field" value={o.align} onchange={(e) => shapeTool.set({ align: e.currentTarget.value as StrokeAlign })}>
        {#each ['inside', 'center', 'outside'] as a (a)}<option value={a} selected={a === o.align}>{t(`shape.align.${a}` as Key)}</option>{/each}
      </select>
    </div>
  {/if}
  <div class="row">
    <span>{t('shape.dash')}</span>
    <select class="field" value={o.dash} onchange={(e) => shapeTool.set({ dash: e.currentTarget.value as Dash })}>
      {#each ['solid', 'dash', 'dot'] as d (d)}<option value={d} selected={d === o.dash}>{t(`shape.dash.${d}` as Key)}</option>{/each}
    </select>
  </div>
</section>

<style>
  .shape { padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; }
  h3 { margin: 0; font-size: var(--fs-1); font-weight: 600; }
  h4 { margin: 6px 0 0; font-size: var(--fs-0); font-weight: 600; color: var(--fg-2); }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px 10px; }
  .grid label { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 4px; color: var(--fg-2); font-size: var(--fs-0); }
  .grid input { width: 100%; min-width: 0; height: 24px; text-align: center; }
  .unit { color: var(--fg-3); font-size: var(--fs-0); min-width: 10px; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 8px; color: var(--fg-2); font-size: var(--fs-0); }
  .row input.field { width: 56px; height: 24px; text-align: center; }
  .check { display: flex; align-items: center; gap: 6px; }
  .swatch { width: 36px; height: 22px; padding: 0; border: 1px solid var(--line-strong); border-radius: var(--radius-sm); background: none; }
  .swatch:disabled { opacity: 0.35; }
  select.field { height: 24px; }
  .note { color: var(--fg-3); }
</style>
