<script lang="ts">
  import { app, type ToolId } from '../state.svelte';
  import { TOOL_GROUPS, type ToolGroup } from '../tools';
  import { t } from '../i18n/index.svelte';
  import Icon from '../icons/Icon.svelte';

  // The tool each group shows (its last-used member).
  let shown = $state<Record<string, ToolId>>(Object.fromEntries(TOOL_GROUPS.map((g) => [g.code, g.tools[0].id])));
  let menuFor = $state<string | null>(null);
  let pressTimer = 0;

  $effect(() => {
    const g = TOOL_GROUPS.find((g) => g.tools.some((tl) => tl.id === app.tool));
    if (g) shown[g.code] = app.tool;
  });

  function current(g: ToolGroup) {
    return g.tools.find((tl) => tl.id === shown[g.code]) ?? g.tools[0];
  }

  function down(g: ToolGroup) {
    if (g.tools.length < 2) return;
    pressTimer = window.setTimeout(() => (menuFor = g.code), 350);
  }

  function up(g: ToolGroup) {
    clearTimeout(pressTimer);
    if (menuFor !== g.code) app.tool = current(g).id;
  }
</script>

<nav class="toolbar" aria-label="tools">
  {#each TOOL_GROUPS as g, i (g.code)}
    {@const tool = current(g)}
    {#if i === 1 || i === 4 || i === 6 || i === 11 || i === 12}<div class="gap"></div>{/if}
    <div class="slot">
      <button
        class="tool"
        aria-pressed={app.tool === tool.id}
        title={`${t(tool.title)} (${g.letter})`}
        onpointerdown={() => down(g)}
        onpointerup={() => up(g)}
        oncontextmenu={(e) => { e.preventDefault(); if (g.tools.length > 1) menuFor = g.code; }}
      >
        <Icon name={tool.icon} size={20} />
        {#if g.tools.length > 1}<span class="more"></span>{/if}
      </button>
      {#if menuFor === g.code}
        <div class="flyout" role="menu">
          {#each g.tools as tl (tl.id)}
            <button class="opt" role="menuitem" onclick={() => { app.tool = tl.id; menuFor = null; }}>
              <Icon name={tl.icon} size={18} /><span>{t(tl.title)}</span><kbd>{g.letter}</kbd>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/each}

  <div class="colors">
    <button class="swatch fg" style:background={app.fg} title={t('tool.foreground')} aria-label={t('tool.foreground')}></button>
    <button class="swatch bg" style:background={app.bg} title={t('tool.background')} aria-label={t('tool.background')}></button>
    <button class="mini swap" title={`${t('tool.swapColors')} (X)`} onclick={() => ([app.fg, app.bg] = [app.bg, app.fg])}><Icon name="swap" size={12} /></button>
    <button class="mini reset" title={`${t('tool.defaultColors')} (D)`} onclick={() => { app.fg = '#000000'; app.bg = '#ffffff'; }}><Icon name="defaultColors" size={12} /></button>
  </div>
</nav>

<svelte:window onpointerdown={(e) => { if (menuFor && !(e.target as Element).closest('.flyout')) menuFor = null; }} />

<style>
  .toolbar { display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 6px 0; width: var(--toolbar-w); background: var(--bg-1); border-inline-end: 1px solid var(--line); overflow-y: auto; overflow-x: visible; }
  .gap { height: 6px; flex: none; }
  .slot { position: relative; }
  .tool { position: relative; display: grid; place-items: center; width: 34px; height: 32px; border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--fg-1); cursor: default; }
  .tool:hover { background: var(--bg-3); color: var(--fg-0); }
  .tool[aria-pressed='true'] { background: var(--bg-4); color: var(--accent); }
  .more { position: absolute; bottom: 3px; inset-inline-end: 3px; width: 0; height: 0; border-inline-start: 4px solid transparent; border-bottom: 4px solid var(--fg-3); }
  .flyout { position: fixed; z-index: 50; margin-inline-start: 40px; margin-top: -32px; min-width: 190px; padding: 4px; background: var(--bg-2); border: 1px solid var(--line-strong); border-radius: var(--radius); box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45); }
  .opt { display: flex; align-items: center; gap: 10px; width: 100%; height: 30px; padding: 0 8px; border: 0; border-radius: var(--radius-sm); background: transparent; text-align: start; cursor: default; }
  .opt:hover { background: var(--bg-4); }
  .opt span { flex: 1; }
  kbd { font: inherit; color: var(--fg-3); }
  .colors { position: relative; width: 36px; height: 44px; margin: auto 0 4px; flex: none; }
  .swatch { position: absolute; width: 22px; height: 22px; border-radius: 3px; border: 1.5px solid var(--bg-0); box-shadow: 0 0 0 1px var(--line-strong); padding: 0; }
  .fg { top: 4px; inset-inline-start: 2px; z-index: 1; }
  .bg { bottom: 0; inset-inline-end: 2px; }
  .mini { position: absolute; display: grid; place-items: center; width: 14px; height: 14px; padding: 0; border: 0; background: transparent; color: var(--fg-2); }
  .swap { top: 0; inset-inline-end: 0; }
  .reset { bottom: 0; inset-inline-start: 0; }
</style>
