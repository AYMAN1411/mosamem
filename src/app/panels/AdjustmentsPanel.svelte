<script lang="ts">
  import { app } from '../state.svelte';
  import { t, type Key } from '../i18n/index.svelte';
  import { ADJUSTMENT_KINDS } from '../../engine/adjust';
  import { adjust } from '../actions';
  import Icon from '../icons/Icon.svelte';
  import { adjustmentIcon } from './adjust/icons';
</script>

{#if app.active}
  <div class="adjustments">
    <p class="hint">{t('adj.add')}</p>
    <div class="grid">
      {#each ADJUSTMENT_KINDS as k (k)}
        <button class="icon-btn kind" title={t(`adj.${k}` as Key)} aria-label={t(`adj.${k}` as Key)} onclick={() => adjust.add(k)}>
          <Icon name={adjustmentIcon(k)} size={20} />
        </button>
      {/each}
    </div>
    <p class="hint small">{t('adj.panelHint')}</p>
  </div>
{:else}
  <div class="empty">{t('panel.empty')}</div>
{/if}

<style>
  .adjustments { padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(34px, 1fr)); gap: 4px; }
  .kind { width: 100%; height: 34px; }
  .hint { margin: 0; color: var(--fg-2); font-size: var(--fs-1); }
  .hint.small { color: var(--fg-3); font-size: var(--fs-0); line-height: 1.6; }
  .empty { padding: 16px; color: var(--fg-3); }
</style>
