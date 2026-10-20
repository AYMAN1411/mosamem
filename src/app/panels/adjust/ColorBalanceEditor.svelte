<script lang="ts">
  import type { Adjustment } from '../../../engine/adjust';
  import { t } from '../../i18n/index.svelte';
  import Slider from './Slider.svelte';

  type B = Extract<Adjustment, { kind: 'colorBalance' }>;
  let { value, onchange }: { value: B; onchange: (next: B, live: boolean) => void } = $props();

  const TONES = ['shadows', 'midtones', 'highlights'] as const;
  let tone = $state<(typeof TONES)[number]>('midtones');
  const v = $derived(value[tone]);

  const AXES = [
    { ends: ['adj.cyan', 'adj.red'], track: 'linear-gradient(90deg, #0cc, #888, #d22)' },
    { ends: ['adj.magenta', 'adj.green'], track: 'linear-gradient(90deg, #c0c, #888, #2b2)' },
    { ends: ['adj.yellow', 'adj.blue'], track: 'linear-gradient(90deg, #dd0, #888, #33e)' },
  ] as const;

  function set(i: number, x: number, live: boolean) {
    onchange({ ...value, [tone]: v.map((c, k) => (k === i ? x : c)) }, live);
  }
</script>

<div class="cb">
  <div class="tones" role="radiogroup" aria-label={t('adj.tone')}>
    {#each TONES as k (k)}
      <label><input type="radio" name="tone" value={k} bind:group={tone} />{t(`adj.${k}`)}</label>
    {/each}
  </div>
  {#each AXES as a, i (i)}
    <Slider label="" value={v[i]} min={-100} max={100} track={a.track} ends={[t(a.ends[0]), t(a.ends[1])]} onchange={(x, live) => set(i, x, live)} />
  {/each}
  <label class="check"><input type="checkbox" checked={value.preserveLuminosity} onchange={(e) => onchange({ ...value, preserveLuminosity: e.currentTarget.checked }, false)} />{t('adj.preserveLuminosity')}</label>
</div>

<style>
  .cb { display: flex; flex-direction: column; gap: 8px; }
  .tones { display: flex; flex-wrap: wrap; gap: 4px 12px; }
  .tones label, .check { display: flex; align-items: center; gap: 5px; color: var(--fg-1); font-size: var(--fs-1); }
</style>
