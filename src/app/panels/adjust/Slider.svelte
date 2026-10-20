<script lang="ts">
  // A labelled number with a slider under it. `onchange(value, live)`: live = true while dragging.
  // The track runs left to right in both languages, like the histogram above it.
  import { fmt } from '../../i18n/index.svelte';

  let {
    label,
    value,
    min,
    max,
    step = 1,
    decimals = 0,
    track = '',
    ends = null,
    onchange,
  }: {
    label: string;
    value: number;
    min: number;
    max: number;
    step?: number;
    decimals?: number;
    /** CSS background for the track (a gradient for color sliders). */
    track?: string;
    /** Labels under the two ends (left, right), e.g. Cyan — Red. */
    ends?: [string, string] | null;
    onchange: (v: number, live: boolean) => void;
  } = $props();

  const clamp = (v: number) => Math.max(min, Math.min(max, v));
  const shown = $derived(decimals ? value.toFixed(decimals) : fmt(Math.round(value)));
</script>

<div class="slider">
  <div class="top">
    <span class="label">{label}</span>
    <input
      class="field num"
      value={shown}
      onfocus={(e) => e.currentTarget.select()}
      onchange={(e) => {
        const n = parseFloat(e.currentTarget.value.replace(/[٫,]/, '.'));
        if (!isNaN(n)) onchange(clamp(n), false);
        else e.currentTarget.value = shown;
      }}
    />
  </div>
  <input
    type="range"
    dir="ltr"
    {min}
    {max}
    {step}
    {value}
    class:colored={!!track}
    style:--track={track || null}
    oninput={(e) => onchange(Number(e.currentTarget.value), true)}
    onchange={(e) => onchange(Number(e.currentTarget.value), false)}
  />
  {#if ends}
    <div class="ends" dir="ltr"><span>{ends[0]}</span><span>{ends[1]}</span></div>
  {/if}
</div>

<style>
  .slider { display: flex; flex-direction: column; gap: 2px; }
  .top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .label { color: var(--fg-1); font-size: var(--fs-1); }
  input.field { width: 64px; height: 22px; padding: 0 4px; text-align: center; font-size: var(--fs-0); }
  input[type='range'] { width: 100%; min-width: 0; margin: 0; accent-color: var(--accent); }
  input.colored { appearance: none; height: 14px; background: transparent; }
  input.colored::-webkit-slider-runnable-track { height: 6px; border-radius: 3px; background: var(--track); box-shadow: 0 0 0 1px var(--line-strong); }
  input.colored::-webkit-slider-thumb { appearance: none; width: 12px; height: 12px; margin-top: -3px; border-radius: 50%; background: var(--fg-0); box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.6); }
  .ends { display: flex; justify-content: space-between; color: var(--fg-3); font-size: var(--fs-0); }
</style>
