<script lang="ts">
  import { app } from '../state.svelte';
  import { t, fmt, type Key } from '../i18n/index.svelte';
  import { settingsFor } from '../canvas/paintTools';
  import { TOOLS } from '../tools';

  // Our own preset list: plain round tips at common sizes.
  const presets: { d: number; h: number; flow?: number; spacing?: number; name: Key }[] = [
    { d: 5, h: 1, name: 'brushes.hard' },
    { d: 13, h: 1, name: 'brushes.hard' },
    { d: 30, h: 1, name: 'brushes.hard' },
    { d: 100, h: 1, name: 'brushes.hard' },
    { d: 9, h: 0, name: 'brushes.soft' },
    { d: 30, h: 0, name: 'brushes.soft' },
    { d: 100, h: 0, name: 'brushes.soft' },
    { d: 300, h: 0, name: 'brushes.soft' },
    { d: 65, h: 0, flow: 0.15, spacing: 0.05, name: 'brushes.airbrush' },
    { d: 200, h: 0, flow: 0.1, spacing: 0.05, name: 'brushes.airbrush' },
    { d: 500, h: 0.2, name: 'brushes.soft' },
    { d: 1000, h: 0, name: 'brushes.soft' },
  ];
  const s = $derived(settingsFor(app.tool));

  function apply(p: (typeof presets)[number]) {
    if (!s) return;
    s.diameter = p.d;
    s.hardness = p.h;
    s.flow = p.flow ?? 1;
    s.spacing = p.spacing ?? 0.1;
  }

  function dot(p: (typeof presets)[number]) {
    const r = Math.min(16, 3 + Math.sqrt(p.d) * 0.9);
    const inner = Math.round(p.h * 100);
    return `width:${r * 2}px;height:${r * 2}px;background:radial-gradient(circle, var(--fg-0) ${inner}%, transparent 100%)`;
  }
</script>

{#if s}
  <div class="brushes">
    <div class="current">{t('brushes.current')}: <strong>{t(TOOLS[app.tool].title)}</strong></div>
    <div class="grid" role="listbox" aria-label={t('brushes.presets')}>
      {#each presets as p, i (i)}
        <button class="preset" role="option" aria-selected={s.diameter === p.d && s.hardness === p.h} title={`${t(p.name)} ${p.d}px`} onclick={() => apply(p)}>
          <span class="dot" style={dot(p)}></span>
          <span class="num">{fmt(p.d)}</span>
        </button>
      {/each}
    </div>
    <label class="slider">{t('opt.size')}
      <input type="range" min="1" max="1000" value={s.diameter} oninput={(e) => (s.diameter = Number(e.currentTarget.value))} />
      <span class="num">{fmt(s.diameter)} px</span>
    </label>
    <label class="slider">{t('opt.hardness')}
      <input type="range" min="0" max="100" value={Math.round(s.hardness * 100)} oninput={(e) => (s.hardness = Number(e.currentTarget.value) / 100)} />
      <span class="num">{fmt(s.hardness * 100)}%</span>
    </label>
    <label class="slider">{t('opt.flow')}
      <input type="range" min="1" max="100" value={Math.round(s.flow * 100)} oninput={(e) => (s.flow = Number(e.currentTarget.value) / 100)} />
      <span class="num">{fmt(s.flow * 100)}%</span>
    </label>
    <label class="slider">{t('opt.smoothing')}
      <input type="range" min="0" max="100" value={Math.round(s.smoothing * 100)} oninput={(e) => (s.smoothing = Number(e.currentTarget.value) / 100)} />
      <span class="num">{fmt(s.smoothing * 100)}%</span>
    </label>
  </div>
{:else}
  <div class="empty">{t('brushes.noTool')}</div>
{/if}

<style>
  .brushes { padding: 10px 12px; display: flex; flex-direction: column; gap: 10px; }
  .current { color: var(--fg-2); font-size: var(--fs-0); }
  .grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 4px; }
  .preset { display: flex; flex-direction: column; align-items: center; justify-content: flex-end; gap: 3px; height: 54px; padding: 4px; border: 1px solid var(--line); border-radius: 6px; background: var(--bg-1); }
  .preset[aria-selected='true'] { border-color: var(--accent); }
  .dot { border-radius: 50%; }
  .preset .num { font-size: var(--fs-0); color: var(--fg-2); }
  .slider { display: grid; grid-template-columns: 70px minmax(0, 1fr) 56px; align-items: center; gap: 8px; color: var(--fg-1); font-size: var(--fs-1); }
  .slider input { accent-color: var(--accent); width: 100%; min-width: 0; }
  .slider .num { text-align: end; color: var(--fg-2); }
  .empty { padding: 16px; color: var(--fg-3); }
</style>
