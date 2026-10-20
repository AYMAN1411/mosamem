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
  import UnsavedDialog from './dialogs/UnsavedDialog.svelte';
  import RecoveryDialog from './dialogs/RecoveryDialog.svelte';
  import ModifyDialog from './dialogs/ModifyDialog.svelte';
  import RefinePanel from './dialogs/RefinePanel.svelte';
  import FilterPanel from './dialogs/FilterPanel.svelte';
  import ImageSizeDialog from './dialogs/ImageSizeDialog.svelte';
  import CanvasSizeDialog from './dialogs/CanvasSizeDialog.svelte';
  import LayerStylePanel from './dialogs/LayerStylePanel.svelte';
  import AiModelDialog from './dialogs/AiModelDialog.svelte';
  import AiModelsDialog from './dialogs/AiModelsDialog.svelte';
  import SmartFillDialog from './dialogs/SmartFillDialog.svelte';
  import { ai, prepareObjectSelect } from './ai.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { engine, type RecoveredFile } from '../engine/bridge';
  import { app } from './state.svelte';
  import { i18n } from './i18n/index.svelte';
  import { buildMenu } from './menu';
  import { openPaths, pendingClose } from './actions';
  import { installShortcuts } from './shortcuts';
  import { installPressureBridge } from './canvas/pressure';

  let dropping = $state(false);

  // the Object Selection tool reads the picture ahead of the first click
  $effect(() => {
    void [app.tool, app.active?.info.version, app.sel.allLayers, ai.packs];
    const id = setTimeout(prepareObjectSelect, 400);
    return () => clearTimeout(id);
  });
  onMount(() => void ai.refresh());
  let recovered = $state<RecoveredFile[]>([]);

  onMount(() => {
    installShortcuts();
    installPressureBridge();
    engine.recoveryList().then((files) => {
      if (files.length) {
        recovered = files;
        app.dialog = 'recovery';
      }
    });
    // closing the window asks about unsaved documents first
    const win = getCurrentWindow();
    const unClose = win.onCloseRequested((e) => {
      const dirty = app.docs.filter((d) => d.info.dirty).map((d) => d.info.id);
      if (!dirty.length) return;
      e.preventDefault();
      pendingClose.docs = dirty;
      pendingClose.then = () => void win.destroy();
      app.activeId = dirty[0];
      app.dialog = 'unsaved';
    });
    const un = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === 'over' || e.payload.type === 'enter') dropping = true;
      else if (e.payload.type === 'leave') dropping = false;
      else if (e.payload.type === 'drop') {
        dropping = false;
        openPaths(e.payload.paths);
      }
    });
    return () => {
      void un.then((f) => f());
      void unClose.then((f) => f());
    };
  });

  // rebuild the native menu when its labels or check states change
  $effect(() => {
    const a = app.active;
    void [i18n.locale, app.showRulers, app.showGrid, app.showGuides, app.showPixelGrid, a != null, a?.info.history.current, a?.info.history.labels.length, a?.info.hasSelection, a?.info.hasLastSelection, a?.info.active, a?.info.version];
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
{#if app.dialog === 'unsaved'}<UnsavedDialog />{/if}
{#if app.dialog === 'recovery'}<RecoveryDialog files={recovered} />{/if}
{#if app.dialog === 'modify'}<ModifyDialog />{/if}
{#if app.dialog === 'refine'}<RefinePanel />{/if}
{#if app.dialog === 'filter'}<FilterPanel />{/if}
{#if app.dialog === 'imageSize' && app.active}<ImageSizeDialog />{/if}
{#if app.dialog === 'canvasSize' && app.active}<CanvasSizeDialog />{/if}
{#if app.dialog === 'layerStyle' && app.active}<LayerStylePanel />{/if}
{#if app.dialog === 'aiModel' && ai.asking}<AiModelDialog />{/if}
{#if app.dialog === 'aiModels'}<AiModelsDialog />{/if}
{#if app.dialog === 'smartFill' && app.active}<SmartFillDialog />{/if}
{#if ai.working}
  <div class="working" role="status"><span class="spin"></span>{ai.working}</div>
{/if}
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
  .working { position: fixed; top: calc(var(--optionsbar-h) + 44px); left: 50%; transform: translateX(-50%); z-index: 94; display: flex; align-items: center; gap: 10px; padding: 8px 16px; border-radius: 999px; background: var(--bg-4); border: 1px solid var(--line-strong); box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4); color: var(--fg-1); }
  .spin { width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--line-strong); border-top-color: var(--accent); animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
