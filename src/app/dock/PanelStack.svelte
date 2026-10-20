<script lang="ts">
  import { dock, PANELS, type PanelId, type Stack } from './layout.svelte';
  import PanelBody from './PanelBody.svelte';
  import Icon from '../icons/Icon.svelte';
  import { t } from '../i18n/index.svelte';

  let { stack, hot, onTabDown }: { stack: Stack; hot: boolean; onTabDown: (p: PanelId, e: PointerEvent) => void } = $props();

  function toggle() {
    stack.collapsed = !stack.collapsed;
    dock.save();
  }
</script>

<section class="stack" class:collapsed={stack.collapsed} style:flex-grow={stack.collapsed ? 0 : stack.grow}>
  <div class="tabs" class:hot data-drop={`stack:${stack.id}`} ondblclick={toggle} role="tablist" tabindex="-1">
    {#each stack.panels as p (p)}
      <button
        class="tab"
        role="tab"
        aria-selected={stack.active === p}
        onpointerdown={(e) => onTabDown(p, e)}
        onclick={() => {
          stack.active = p;
          stack.collapsed = false;
          dock.save();
        }}
      >
        {t(PANELS[p].title)}
      </button>
    {/each}
    <span class="spacer"></span>
    <button class="icon-btn small" title={stack.collapsed ? t('panel.expand') : t('panel.collapse')} onclick={toggle}>
      <span class="chev" class:up={!stack.collapsed}><Icon name="chevronDown" size={14} /></span>
    </button>
  </div>
  {#if !stack.collapsed}
    <div class="body"><PanelBody panel={stack.active} /></div>
  {/if}
</section>

<style>
  .stack { display: flex; flex-direction: column; min-height: 0; flex-basis: 0; background: var(--bg-2); border-block: 1px solid var(--line); }
  .stack.collapsed { flex-basis: auto; }
  .tabs { display: flex; align-items: center; gap: 2px; height: 30px; padding-inline: 4px; background: var(--bg-1); border-bottom: 1px solid var(--line); flex: none; }
  .tabs.hot { box-shadow: inset 0 0 0 2px var(--accent); }
  .tab { height: 24px; padding: 0 10px; border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--fg-2); font-size: var(--fs-1); cursor: default; }
  .tab:hover { color: var(--fg-0); }
  .tab[aria-selected='true'] { background: var(--bg-2); color: var(--fg-0); font-weight: 600; }
  .spacer { flex: 1; }
  .small { width: 22px; height: 22px; }
  .chev { display: inline-grid; transition: transform 0.12s; }
  .chev.up { transform: rotate(180deg); }
  .body { flex: 1; min-height: 0; overflow: auto; }
</style>
