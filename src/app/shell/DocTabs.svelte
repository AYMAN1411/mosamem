<script lang="ts">
  import { app } from '../state.svelte';
  import { closeDocument } from '../actions';
  import Icon from '../icons/Icon.svelte';
</script>

{#if app.docs.length}
  <div class="tabs" role="tablist">
    {#each app.docs as d (d.info.id)}
      <div class="tab" class:active={d.info.id === app.activeId} role="tab" aria-selected={d.info.id === app.activeId}>
        <button class="label" onclick={() => (app.activeId = d.info.id)}>{d.info.name}</button>
        <button class="close icon-btn" onclick={() => closeDocument(d.info.id)} aria-label="close"><Icon name="close" size={12} /></button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .tabs { display: flex; height: var(--tabs-h); background: var(--bg-1); border-bottom: 1px solid var(--line); overflow-x: auto; }
  .tab { display: flex; align-items: center; gap: 2px; max-width: 240px; padding-inline: 10px 4px; border-inline-end: 1px solid var(--line); color: var(--fg-2); }
  .tab.active { background: var(--bg-0); color: var(--fg-0); }
  .label { border: 0; background: none; padding: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: default; }
  .close { width: 20px; height: 20px; opacity: 0.6; }
  .close:hover { opacity: 1; }
</style>
