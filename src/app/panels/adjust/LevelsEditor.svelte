<script lang="ts">
  import type { Adjustment, Histogram as Hist, Levels } from '../../../engine/adjust';
  import { t } from '../../i18n/index.svelte';
  import Histogram from './Histogram.svelte';

  type L = Extract<Adjustment, { kind: 'levels' }>;
  let { value, hist, onchange }: { value: L; hist: Hist | null; onchange: (next: L, live: boolean) => void } = $props();

  let channel = $state(0);
  const ch = $derived(value.channels[channel]);
  const histData = $derived(hist ? [hist.luminosity, hist.red, hist.green, hist.blue][channel] : null);
  const histColor = $derived(['var(--fg-2)', '#e5534b', '#57ab5a', '#539bf5'][channel]);

  function set(patch: Partial<Levels>, live: boolean) {
    const channels = value.channels.map((c, i) => (i === channel ? { ...c, ...patch } : c)) as L['channels'];
    onchange({ ...value, channels }, live);
  }

  // the gray handle sits where an input maps to mid-gray: t = 0.5^gamma between black and white
  const grayPos = $derived(ch.inBlack + (ch.inWhite - ch.inBlack) * Math.pow(0.5, ch.gamma));

  function drag(e: PointerEvent, which: 'inBlack' | 'gray' | 'inWhite' | 'outBlack' | 'outWhite') {
    const bar = (e.currentTarget as HTMLElement).parentElement!;
    const r = bar.getBoundingClientRect();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const at = (ev: PointerEvent) => Math.max(0, Math.min(255, Math.round(((ev.clientX - r.left) / r.width) * 255)));
    const apply = (ev: PointerEvent, live: boolean) => {
      const x = at(ev);
      if (which === 'inBlack') set({ inBlack: Math.min(x, ch.inWhite - 2) }, live);
      else if (which === 'inWhite') set({ inWhite: Math.max(x, ch.inBlack + 2) }, live);
      else if (which === 'gray') {
        const tt = Math.max(0.01, Math.min(0.99, (x - ch.inBlack) / Math.max(1, ch.inWhite - ch.inBlack)));
        set({ gamma: Math.round(Math.max(0.01, Math.min(9.99, Math.log(tt) / Math.log(0.5))) * 100) / 100 }, live);
      } else set({ [which]: x }, live);
    };
    const move = (ev: PointerEvent) => apply(ev, true);
    const up = (ev: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      apply(ev, false);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  function num(e: Event & { currentTarget: HTMLInputElement }, key: keyof Levels, lo: number, hi: number) {
    const n = parseFloat(e.currentTarget.value);
    if (isNaN(n)) return;
    set({ [key]: Math.max(lo, Math.min(hi, n)) }, false);
  }
</script>

<div class="levels">
  <select class="field" bind:value={channel} aria-label={t('adj.channel')}>
    <option value={0}>{t('adj.ch.rgb')}</option>
    <option value={1}>{t('adj.ch.red')}</option>
    <option value={2}>{t('adj.ch.green')}</option>
    <option value={3}>{t('adj.ch.blue')}</option>
  </select>
  <span class="cap">{t('adj.input')}</span>
  <Histogram data={histData} color={histColor} />
  <div class="bar input" dir="ltr">
    <button class="h black" style:left={`${(ch.inBlack / 255) * 100}%`} onpointerdown={(e) => drag(e, 'inBlack')} aria-label="black"></button>
    <button class="h gray" style:left={`${(grayPos / 255) * 100}%`} onpointerdown={(e) => drag(e, 'gray')} aria-label="gamma"></button>
    <button class="h white" style:left={`${(ch.inWhite / 255) * 100}%`} onpointerdown={(e) => drag(e, 'inWhite')} aria-label="white"></button>
  </div>
  <div class="nums" dir="ltr">
    <input class="field num" value={ch.inBlack} onchange={(e) => num(e, 'inBlack', 0, ch.inWhite - 2)} />
    <input class="field num" value={ch.gamma.toFixed(2)} onchange={(e) => num(e, 'gamma', 0.01, 9.99)} aria-label={t('adj.gamma')} />
    <input class="field num" value={ch.inWhite} onchange={(e) => num(e, 'inWhite', ch.inBlack + 2, 255)} />
  </div>
  <span class="cap">{t('adj.output')}</span>
  <div class="bar output" dir="ltr">
    <button class="h black" style:left={`${(ch.outBlack / 255) * 100}%`} onpointerdown={(e) => drag(e, 'outBlack')} aria-label="black"></button>
    <button class="h white" style:left={`${(ch.outWhite / 255) * 100}%`} onpointerdown={(e) => drag(e, 'outWhite')} aria-label="white"></button>
  </div>
  <div class="nums two" dir="ltr">
    <input class="field num" value={ch.outBlack} onchange={(e) => num(e, 'outBlack', 0, 255)} />
    <input class="field num" value={ch.outWhite} onchange={(e) => num(e, 'outWhite', 0, 255)} />
  </div>
</div>

<style>
  .levels { display: flex; flex-direction: column; gap: 6px; }
  .bar { position: relative; height: 12px; margin: 0 6px; border-radius: 2px; background: linear-gradient(90deg, #000, #fff); }
  .bar.input { margin-top: -2px; height: 6px; background: none; border-top: 1px solid var(--line); }
  .h { position: absolute; top: 0; width: 12px; height: 11px; padding: 0; margin-left: -6px; border: 1px solid #000; clip-path: polygon(50% 0, 100% 100%, 0 100%); cursor: ew-resize; }
  .bar.output .h { top: 12px; }
  .bar.output { margin-bottom: 12px; }
  .h.black { background: #111; border-color: #888; }
  .h.gray { background: #888; }
  .h.white { background: #fff; }
  .nums { display: flex; justify-content: space-between; margin-top: 8px; }
  .nums input { width: 56px; height: 22px; text-align: center; font-size: var(--fs-0); padding: 0 2px; }
  .cap { color: var(--fg-2); font-size: var(--fs-0); margin-top: 4px; }
</style>
