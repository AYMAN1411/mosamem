<script lang="ts">
  import { app } from '../state.svelte';
  import { t } from '../i18n/index.svelte';
  import { historyText } from '../history';
  import { history } from '../actions';
  import Icon from '../icons/Icon.svelte';

  const h = $derived(app.active?.info.history);
  let list = $state<HTMLOListElement>();

  // keep the current step in view
  $effect(() => {
    void h?.current;
    list?.querySelector('.current')?.scrollIntoView({ block: 'nearest' });
  });
</script>

{#if h}
  <ol class="steps" bind:this={list}>
    {#each h.labels as label, i (i)}
      <li>
        <button class="step" class:current={i === h.current} class:future={i > h.current} onclick={() => history.jump(i)} title={label.arg ?? ''}>
          <Icon name={i === 0 ? 'image' : 'history'} size={14} />
          <span>{historyText(label)}</span>
        </button>
      </li>
    {/each}
  </ol>
{:else}
  <div class="empty">{t('panel.empty')}</div>
{/if}

<style>
  .steps { list-style: none; margin: 0; padding: 4px; }
  .step { display: flex; align-items: center; gap: 8px; width: 100%; height: 28px; padding: 0 8px; border: 0; border-radius: var(--radius-sm); background: none; color: var(--fg-1); text-align: start; cursor: default; }
  .step:hover { background: var(--bg-3); }
  .step.current { background: var(--bg-4); color: var(--fg-0); font-weight: 600; }
  .step.future { color: var(--fg-3); }
  .empty { padding: 16px; color: var(--fg-3); }
</style>
