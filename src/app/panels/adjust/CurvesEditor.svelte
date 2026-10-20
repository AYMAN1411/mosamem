<script lang="ts">
  import type { Adjustment, Histogram as Hist } from '../../../engine/adjust';
  import { monotoneSpline } from '../../../engine/spline';
  import { t } from '../../i18n/index.svelte';

  type C = Extract<Adjustment, { kind: 'curves' }>;
  let { value, hist, onchange }: { value: C; hist: Hist | null; onchange: (next: C, live: boolean) => void } = $props();

  let channel = $state(0);
  let selected = $state<number | null>(null);
  let svg = $state<SVGSVGElement>();
  const pts = $derived(value.channels[channel]);
  const color = $derived(['var(--fg-0)', '#e5534b', '#57ab5a', '#539bf5'][channel]);

  const curve = $derived.by(() => {
    const f = monotoneSpline(pts);
    let d = '';
    for (let x = 0; x <= 255; x += 1) d += `${x ? 'L' : 'M'}${x} ${255 - f(x)}`;
    return d;
  });

  const histPath = $derived.by(() => {
    if (!hist) return '';
    const data = [hist.luminosity, hist.red, hist.green, hist.blue][channel];
    const sorted = data.filter((v) => v > 0).sort((a, b) => a - b);
    const cap = sorted.length ? sorted[Math.floor(sorted.length * 0.98)] || 1 : 1;
    let d = 'M0 255';
    for (let i = 0; i < 256; i++) d += `L${i} ${255 - Math.min(1, data[i] / cap) * 254}`;
    return d + 'L256 255Z';
  });

  function setPoints(next: [number, number][], live: boolean) {
    const channels = value.channels.map((c, i) => (i === channel ? next : c));
    onchange({ ...value, channels }, live);
  }

  function toCurve(e: PointerEvent): [number, number] {
    const r = svg!.getBoundingClientRect();
    const x = Math.round(((e.clientX - r.left) / r.width) * 255);
    const y = Math.round(255 - ((e.clientY - r.top) / r.height) * 255);
    return [Math.max(0, Math.min(255, x)), Math.max(0, Math.min(255, y))];
  }

  function startDrag(e: PointerEvent, index: number, list: [number, number][]) {
    selected = index;
    svg!.setPointerCapture(e.pointerId);
    let current = list;
    const place = (ev: PointerEvent) => {
      const [x, y] = toCurve(ev);
      // stay between the neighbours
      const lo = index > 0 ? current[index - 1][0] + 1 : 0;
      const hi = index < current.length - 1 ? current[index + 1][0] - 1 : 255;
      current = current.map((p, i) => (i === index ? [Math.max(lo, Math.min(hi, x)), y] : p));
      return current;
    };
    const move = (ev: PointerEvent) => setPoints(place(ev), true);
    const up = (ev: PointerEvent) => {
      svg!.removeEventListener('pointermove', move);
      svg!.removeEventListener('pointerup', up);
      setPoints(place(ev), false);
    };
    svg!.addEventListener('pointermove', move);
    svg!.addEventListener('pointerup', up);
  }

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    const [x, y] = toCurve(e);
    // grab a point near the pointer, or add one on the curve
    const r = svg!.getBoundingClientRect();
    const tol = (8 / r.width) * 255;
    const near = pts.findIndex((p) => Math.hypot(p[0] - x, p[1] - y) <= tol);
    if (near >= 0) return startDrag(e, near, pts);
    if (pts.length >= 14 || pts.some((p) => p[0] === x)) return;
    const next = [...pts, [x, Math.round(monotoneSpline(pts)(x))] as [number, number]].sort((a, b) => a[0] - b[0]);
    startDrag(e, next.findIndex((p) => p[0] === x), next);
  }

  function remove(index: number) {
    if (pts.length <= 2) return;
    selected = null;
    setPoints(pts.filter((_, i) => i !== index), false);
  }

  function edit(axis: 0 | 1, v: string) {
    if (selected == null) return;
    const n = Math.max(0, Math.min(255, Math.round(parseFloat(v))));
    if (isNaN(n)) return;
    setPoints(pts.map((p, i) => (i === selected ? (axis === 0 ? [n, p[1]] : [p[0], n]) : p)).sort((a, b) => a[0] - b[0]) as [number, number][], false);
  }
</script>

<div class="curves">
  <select class="field" bind:value={channel} onchange={() => (selected = null)} aria-label={t('adj.channel')}>
    <option value={0}>{t('adj.ch.rgb')}</option>
    <option value={1}>{t('adj.ch.red')}</option>
    <option value={2}>{t('adj.ch.green')}</option>
    <option value={3}>{t('adj.ch.blue')}</option>
  </select>
  <svg bind:this={svg} class="plot" viewBox="-4 -4 264 264" onpointerdown={down} role="application" aria-label={t('adj.curves')}>
    <rect x="0" y="0" width="255" height="255" class="bg" />
    <path d={histPath} class="hist" />
    {#each [64, 128, 192] as g (g)}
      <line x1={g} x2={g} y1="0" y2="255" class="grid" />
      <line y1={g} y2={g} x1="0" x2="255" class="grid" />
    {/each}
    <line x1="0" y1="255" x2="255" y2="0" class="grid diag" />
    <path d={curve} fill="none" stroke={color} stroke-width="1.6" vector-effect="non-scaling-stroke" />
    {#each pts as p, i (i)}
      <rect
        x={p[0] - 3.5}
        y={255 - p[1] - 3.5}
        width="7"
        height="7"
        class="pt"
        class:sel={i === selected}
        role="button"
        tabindex="-1"
        ondblclick={(e) => { e.stopPropagation(); remove(i); }}
      />
    {/each}
  </svg>
  <div class="io" dir="ltr">
    <label>{t('adj.in')} <input class="field num" disabled={selected == null} value={selected != null ? pts[selected]?.[0] ?? '' : ''} onchange={(e) => edit(0, e.currentTarget.value)} /></label>
    <label>{t('adj.out')} <input class="field num" disabled={selected == null} value={selected != null ? pts[selected]?.[1] ?? '' : ''} onchange={(e) => edit(1, e.currentTarget.value)} /></label>
  </div>
  <p class="hint">{t('adj.curvesHint')}</p>
</div>

<style>
  .curves { display: flex; flex-direction: column; gap: 6px; }
  .plot { width: 100%; aspect-ratio: 1; touch-action: none; cursor: crosshair; }
  .bg { fill: var(--bg-0); stroke: var(--line); }
  .hist { fill: var(--fg-3); opacity: 0.35; }
  .grid { stroke: var(--line); stroke-width: 0.8; vector-effect: non-scaling-stroke; }
  .diag { stroke-dasharray: 3 3; }
  .pt { fill: var(--bg-0); stroke: var(--fg-0); stroke-width: 1.2; vector-effect: non-scaling-stroke; cursor: move; }
  .pt.sel { fill: var(--fg-0); }
  .io { display: flex; gap: 10px; color: var(--fg-2); font-size: var(--fs-0); }
  .io input { width: 48px; height: 22px; margin-inline-start: 4px; text-align: center; }
  .hint { margin: 0; color: var(--fg-3); font-size: var(--fs-0); line-height: 1.5; }
</style>
