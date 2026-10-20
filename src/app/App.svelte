<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import ToolBar from './shell/ToolBar.svelte';
  import OptionsBar from './shell/OptionsBar.svelte';
  import StatusBar from './shell/StatusBar.svelte';
  import DocTabs from './shell/DocTabs.svelte';
  import Dock from './dock/Dock.svelte';
  import CanvasView from './canvas/CanvasView.svelte';
  import Welcome from './Welcome.svelte';
  import NewDocument from './dialogs/NewDocument.svelte';
  import ExportDialog from './dialogs/ExportDialog.svelte';
  import { app } from './state.svelte';
  import { i18n } from './i18n/index.svelte';
  import { buildMenu } from './menu';
  import { openPaths } from './actions';
  import { installShortcuts } from './shortcuts';

  let dropping = $state(false);

  onMount(() => {
    installShortcuts();
    const un = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === 'over' || e.payload.type === 'enter') dropping = true;
      else if (e.payload.type === 'leave') dropping = false;
      else if (e.payload.type === 'drop') {
        dropping = false;
        openPaths(e.payload.paths);
      }
    });
    return () => void un.then((f) => f());
  });

  // rebuild the native menu when its labels or check states change
  $effect(() => {
    void [i18n.locale, app.showRulers, app.showGrid, app.showGuides, app.showPixelGrid, app.active != null];
    buildMenu();
  });
</script>

<div class="app" dir={i18n.dir}>
  <div class="options"><OptionsBar /></div>
  <div class="tools"><ToolBar /></div>
  <main class="main">
    <DocTabs />
    <div class="canvas">
      <CanvasView />
      {#if !app.docs.length}<Welcome />{/if}
    </div>
  </main>
  <aside class="dock"><Dock /></aside>
  <div class="status"><StatusBar /></div>
  {#if dropping}<div class="drop"></div>{/if}
</div>

{#if app.dialog === 'new'}<NewDocument />{/if}
{#if app.dialog === 'export'}<ExportDialog />{/if}
{#if app.toast}
  <div class="toast" class:error={app.toast.kind === 'error'} role="status">{app.toast.text}</div>
{/if}

<style>
  .app { display: grid; height: 100vh; grid-template-columns: var(--toolbar-w) minmax(0, 1fr) auto; grid-template-rows: var(--optionsbar-h) minmax(0, 1fr) var(--statusbar-h); grid-template-areas: 'options options options' 'tools main dock' 'status status status'; }
  .options { grid-area: options; }
  .tools { grid-area: tools; min-height: 0; display: flex; }
  .main { grid-area: main; display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .canvas { position: relative; flex: 1; min-height: 0; }
  .dock { grid-area: dock; min-height: 0; }
  .status { grid-area: status; }
  .drop { position: fixed; inset: 8px; border: 2px dashed var(--accent); border-radius: 12px; background: var(--accent-soft); pointer-events: none; z-index: 90; }
  .toast { position: fixed; bottom: 40px; left: 50%; transform: translateX(-50%); z-index: 95; padding: 8px 16px; border-radius: 8px; background: var(--bg-4); border: 1px solid var(--line-strong); box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4); }
  .toast.error { border-color: var(--danger); }
</style>
