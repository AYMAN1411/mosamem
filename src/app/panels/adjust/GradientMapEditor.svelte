<script lang="ts">
  import type { Adjustment, GradientStop } from '../../../engine/adjust';
  import { t } from '../../i18n/index.svelte';
  import Icon from '../../icons/Icon.svelte';

  type G = Extract<Adjustment, { kind: 'gradientMap' }>;
  let { value, onchange }: { value: G; onchange: (next: G, live: boolean) => void } = $props();

  const hex = (c: number[]) => `#${c.map((v) => v.toString(16).padStart(2, '0')).join('')}`;
  const rgb = (h: string): [number, number, number] => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
  const sorted = $derived([...value.stops].sort((a, b) => a.pos - b.pos));
  const preview = $derived(`linear-gradient(90deg, ${sorted.map((s) => `${hex(s.color)} ${(s.pos * 100).toFixed(1)}%`).join(', ')})`);

  function setStop(i: number, patch: Partial<GradientStop>, live: boolean) {
    onchange({ ...value, stops: value.stops.map((s, k) => (k === i ? { ...s, ...patch } : s)) }, live);
  }

  function addStop() {
    // in the widest gap, with the color halfway between its neighbours
    let best = 0;
    for (let i = 1; i < sorted.length - 1; i++) if (sorted[i + 1].pos - sorted[i].pos > sorted[best + 1].pos - sorted[best].pos) best = i;
    const [a, b] = [sorted[best], sorted[best + 1] ?? sorted[best]];
    const color = a.color.map((c, k) => Math.round((c + b.color[k]) / 2)) as [number, number, number];
    onchange({ ...value, stops: [...value.stops, { pos: (a.pos + b.pos) / 2, color }] }, false);
  }
</script>

<div class="gm">
  <div class="preview" dir="ltr" style:background={preview}></div>
  <span class="cap">{t('adj.stops')}</span>
  {#each value.stops as s, i (i)}
    <div class="stop">
      <input
        type="color"
        value={hex(s.color)}
        oninput={(e) => setStop(i, { color: rgb(e.currentTarget.value) }, true)}
        onchange={(e) => setStop(i, { color: rgb(e.currentTarget.value) }, false)}
      />
      <input
        type="range"
        dir="ltr"
        min="0"
        max="100"
        value={Math.round(s.pos * 100)}
        oninput={(e) => setStop(i, { pos: Number(e.currentTarget.value) / 100 }, true)}
        onchange={(e) => setStop(i, { pos: Number(e.currentTarget.value) / 100 }, false)}
      />
      <span class="num">{Math.round(s.pos * 100)}%</span>
      <button class="icon-btn small" title={t('adj.removeStop')} disabled={value.stops.length <= 2} onclick={() => onchange({ ...value, stops: value.stops.filter((_, k) => k !== i) }, false)}><Icon name="trash" size={14} /></button>
    </div>
  {/each}
  <button class="btn add" onclick={addStop}><Icon name="plus" size={14} />{t('adj.addStop')}</button>
  <label class="check"><input type="checkbox" checked={value.reverse} onchange={(e) => onchange({ ...value, reverse: e.currentTarget.checked }, false)} />{t('adj.reverse')}</label>
  <label class="check"><input type="checkbox" checked={value.dither} onchange={(e) => onchange({ ...value, dither: e.currentTarget.checked }, false)} />{t('adj.dither')}</label>
</div>

<style>
  .gm { display: flex; flex-direction: column; gap: 6px; }
  .preview { height: 22px; border-radius: var(--radius-sm); box-shadow: 0 0 0 1px var(--line-strong); }
  .cap { color: var(--fg-2); font-size: var(--fs-0); }
  .stop { display: grid; grid-template-columns: 34px minmax(0, 1fr) 38px 24px; align-items: center; gap: 6px; }
  .stop input[type='range'] { width: 100%; min-width: 0; accent-color: var(--accent); }
  .num { color: var(--fg-2); font-size: var(--fs-0); text-align: end; }
  input[type='color'] { width: 34px; height: 22px; padding: 0; border: 1px solid var(--line-strong); border-radius: var(--radius-sm); background: none; }
  .small { width: 24px; height: 24px; }
  .add { align-self: flex-start; height: 24px; padding: 0 8px; font-size: var(--fs-0); }
  .check { display: flex; align-items: center; gap: 6px; color: var(--fg-1); font-size: var(--fs-1); }
</style>
