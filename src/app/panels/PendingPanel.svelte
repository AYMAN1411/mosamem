<script lang="ts">
  import type { PanelId } from '../dock/layout.svelte';
  import { i18n } from '../i18n/index.svelte';

  let { panel }: { panel: PanelId } = $props();

  // Which milestone brings each panel's content (docs/plan.md).
  const milestone: Partial<Record<PanelId, number>> = {};
</script>

<div class="pending">
  {#if i18n.locale === 'ar'}
    يكتمل هذا اللوح في المرحلة {milestone[panel] ?? ''} من الخطة.
  {:else}
    This panel is completed in milestone {milestone[panel] ?? ''} of the plan.
  {/if}
</div>

<style>
  .pending { padding: 16px; color: var(--fg-3); line-height: 1.6; }
</style>
