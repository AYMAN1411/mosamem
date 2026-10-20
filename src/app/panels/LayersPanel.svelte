<script lang="ts">
  import { app } from '../state.svelte';
  import { t } from '../i18n/index.svelte';
  import Icon from '../icons/Icon.svelte';
</script>

{#if app.active}
  <ul class="layers" role="listbox">
    {#each app.active.info.layers as layer (layer.id)}
      <li class="row" class:active={layer.id === app.active.info.active} role="option" aria-selected={layer.id === app.active.info.active}>
        <span class="eye" title={t('layer.visible')}><Icon name={layer.visible ? 'eye' : 'eyeOff'} size={16} /></span>
        <span class="thumb"></span>
        <span class="name">{layer.name}</span>
      </li>
    {/each}
  </ul>
{:else}
  <div class="empty">{t('panel.empty')}</div>
{/if}

<style>
  .layers { list-style: none; margin: 0; padding: 4px; }
  .row { display: flex; align-items: center; gap: 8px; height: 40px; padding: 0 6px; border-radius: var(--radius-sm); color: var(--fg-1); }
  .row.active { background: var(--bg-4); color: var(--fg-0); }
  .eye { display: inline-grid; color: var(--fg-2); }
  .thumb { width: 32px; height: 26px; border-radius: 3px; border: 1px solid var(--line-strong); background: repeating-conic-gradient(#555 0 25%, #777 0 50%) 0 0 / 8px 8px; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { padding: 16px; color: var(--fg-3); }
</style>
