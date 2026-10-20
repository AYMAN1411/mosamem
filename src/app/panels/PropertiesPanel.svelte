<script lang="ts">
  import { app } from '../state.svelte';
  import { t, fmt } from '../i18n/index.svelte';
</script>

{#if app.active}
  {@const info = app.active.info}
  <dl class="props">
    <dt>{t('newdoc.width')}</dt><dd class="num">{fmt(info.width)} px</dd>
    <dt>{t('newdoc.height')}</dt><dd class="num">{fmt(info.height)} px</dd>
    <dt>{t('newdoc.resolution')}</dt><dd class="num">{fmt(info.ppi)} ppi</dd>
  </dl>
{:else}
  <div class="empty">{t('panel.empty')}</div>
{/if}

<style>
  .props { display: grid; grid-template-columns: auto 1fr; gap: 6px 12px; margin: 0; padding: 12px; }
  dt { color: var(--fg-2); }
  dd { margin: 0; text-align: end; }
  .empty { padding: 16px; color: var(--fg-3); }
</style>
