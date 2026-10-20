<script lang="ts">
  import type { PanelId } from './layout.svelte';
  import LayersPanel from '../panels/LayersPanel.svelte';
  import PropertiesPanel from '../panels/PropertiesPanel.svelte';
  import ColorPanel from '../panels/ColorPanel.svelte';
  import NavigatorPanel from '../panels/NavigatorPanel.svelte';
  import PendingPanel from '../panels/PendingPanel.svelte';
  import HistoryPanel from '../panels/HistoryPanel.svelte';

  let { panel }: { panel: PanelId } = $props();
</script>

{#if panel === 'layers'}
  <LayersPanel />
{:else if panel === 'properties'}
  <PropertiesPanel />
{:else if panel === 'color'}
  <ColorPanel />
{:else if panel === 'history'}
  <HistoryPanel />
{:else if panel === 'navigator'}
  <NavigatorPanel />
{:else}
  <PendingPanel {panel} />
{/if}
