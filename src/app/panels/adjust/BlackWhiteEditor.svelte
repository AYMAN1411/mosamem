<script lang="ts">
  import type { Adjustment } from '../../../engine/adjust';
  import { t, type Key } from '../../i18n/index.svelte';
  import Slider from './Slider.svelte';

  type W = Extract<Adjustment, { kind: 'blackWhite' }>;
  let { value, onchange }: { value: W; onchange: (next: W, live: boolean) => void } = $props();

  const NAMES = ['reds', 'yellows', 'greens', 'cyans', 'blues', 'magentas'] as const;
  const HUES = ['#e33', '#ee3', '#3d3', '#3dd', '#33e', '#e3e'];
  const hex = (c: number[]) => `#${c.map((v) => v.toString(16).padStart(2, '0')).join('')}`;
  const rgb = (h: string): [number, number, number] => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];

  function setWeight(i: number, v: number, live: boolean) {
    onchange({ ...value, weights: value.weights.map((w, k) => (k === i ? v : w)) }, live);
  }
</script>

<div class="bw">
  {#each NAMES as n, i (n)}
    <Slider label={t(`adj.range.${n}` as Key)} value={value.weights[i]} min={-200} max={300} track={`linear-gradient(90deg, #000, ${HUES[i]} 40%, #fff)`} onchange={(v, live) => setWeight(i, v, live)} />
  {/each}
  <div class="tint">
    <label class="check"><input type="checkbox" checked={value.tint} onchange={(e) => onchange({ ...value, tint: e.currentTarget.checked }, false)} />{t('adj.tint')}</label>
    <input
      type="color"
      value={hex(value.tintColor)}
      disabled={!value.tint}
      oninput={(e) => onchange({ ...value, tintColor: rgb(e.currentTarget.value) }, true)}
      onchange={(e) => onchange({ ...value, tintColor: rgb(e.currentTarget.value) }, false)}
    />
  </div>
</div>

<style>
  .bw { display: flex; flex-direction: column; gap: 8px; }
  .tint { display: flex; align-items: center; justify-content: space-between; }
  .check { display: flex; align-items: center; gap: 6px; color: var(--fg-1); font-size: var(--fs-1); }
  input[type='color'] { width: 40px; height: 22px; padding: 0; border: 1px solid var(--line-strong); border-radius: var(--radius-sm); background: none; }
</style>
