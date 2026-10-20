<script lang="ts">
  import { dock, PANELS, type Column, type DropTarget, type PanelId } from './layout.svelte';
  import PanelStack from './PanelStack.svelte';
  import PanelBody from './PanelBody.svelte';
  import Icon from '../icons/Icon.svelte';
  import { t } from '../i18n/index.svelte';

  // ---- tab dragging: rearranges panels between stacks and columns ----
  let pending: { panel: PanelId; x: number; y: number } | null = null;

  export function beginTabDrag(panel: PanelId, e: PointerEvent) {
    pending = { panel, x: e.clientX, y: e.clientY };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp, { once: true });
  }

  function targetAt(x: number, y: number): DropTarget | null {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-drop]');
    if (!el) return null;
    const [kind, a, b] = el.dataset.drop!.split(':');
    if (kind === 'stack') return { kind, stack: a };
    if (kind === 'between') return { kind, column: a, index: Number(b) };
    if (kind === 'column') return { kind, index: Number(a) };
    return null;
  }

  function onMove(e: PointerEvent) {
    if (pending && !dock.drag && Math.hypot(e.clientX - pending.x, e.clientY - pending.y) > 5) {
      dock.drag = { panel: pending.panel, x: e.clientX, y: e.clientY, target: null };
    }
    if (dock.drag) dock.drag = { ...dock.drag, x: e.clientX, y: e.clientY, target: targetAt(e.clientX, e.clientY) };
  }

  function onUp() {
    window.removeEventListener('pointermove', onMove);
    if (dock.drag?.target) dock.apply(dock.drag.panel, dock.drag.target);
    dock.drag = null;
    pending = null;
  }

  // ---- column resizing ----
  function beginResize(col: Column, e: PointerEvent) {
    const startX = e.clientX;
    const startW = col.width;
    const rtl = document.documentElement.dir === 'rtl';
    const move = (ev: PointerEvent) => {
      // the handle is on the canvas side; dragging toward the canvas widens the column
      const dx = (ev.clientX - startX) * (rtl ? 1 : -1);
      col.width = Math.max(200, Math.min(520, startW + dx));
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', () => {
      window.removeEventListener('pointermove', move);
      dock.save();
    }, { once: true });
  }

  const isTarget = (d: DropTarget) => JSON.stringify(dock.drag?.target) === JSON.stringify(d);
</script>

<div class="dock" class:dragging={!!dock.drag}>
  <div class="col-drop" data-drop="column:0" class:hot={isTarget({ kind: 'column', index: 0 })}></div>
  {#each dock.columns as col, ci (col.id)}
    {#if col.iconic}
      <div class="icon-col">
        {#each col.stacks as stack, si (stack.id)}
          {#if si > 0}<div class="icon-sep"></div>{/if}
          <div class="icon-group" data-drop={`stack:${stack.id}`} class:hot={isTarget({ kind: 'stack', stack: stack.id })}>
            {#each stack.panels as p (p)}
              <button
                class="icon-btn"
                title={t(PANELS[p].title)}
                aria-pressed={dock.flyout === p}
                onpointerdown={(e) => beginTabDrag(p, e)}
                onclick={() => (dock.flyout = dock.flyout === p ? null : p)}
              >
                <Icon name={PANELS[p].icon} />
              </button>
            {/each}
          </div>
        {/each}
        {#if dock.flyout && col.stacks.some((s) => s.panels.includes(dock.flyout!))}
          <div class="flyout">
            <header>
              <span>{t(PANELS[dock.flyout].title)}</span>
              <button class="icon-btn" onclick={() => (dock.flyout = null)} aria-label="close"><Icon name="close" size={14} /></button>
            </header>
            <div class="flyout-body"><PanelBody panel={dock.flyout} /></div>
          </div>
        {/if}
      </div>
    {:else}
      <div class="column" style:width={`${col.width}px`}>
        <div class="resize" onpointerdown={(e) => beginResize(col, e)} role="separator" aria-orientation="vertical"></div>
        <div class="between" data-drop={`between:${col.id}:0`} class:hot={isTarget({ kind: 'between', column: col.id, index: 0 })}></div>
        {#each col.stacks as stack, si (stack.id)}
          <PanelStack {stack} onTabDown={beginTabDrag} hot={isTarget({ kind: 'stack', stack: stack.id })} />
          <div class="between" data-drop={`between:${col.id}:${si + 1}`} class:hot={isTarget({ kind: 'between', column: col.id, index: si + 1 })}></div>
        {/each}
      </div>
    {/if}
    <div class="col-drop" data-drop={`column:${ci + 1}`} class:hot={isTarget({ kind: 'column', index: ci + 1 })}></div>
  {/each}
</div>

{#if dock.drag}
  <div class="ghost" style:left={`${dock.drag.x + 12}px`} style:top={`${dock.drag.y + 8}px`}>
    <Icon name={PANELS[dock.drag.panel].icon} size={14} />
    {t(PANELS[dock.drag.panel].title)}
  </div>
{/if}

<style>
  .dock { display: flex; height: 100%; min-height: 0; background: var(--bg-1); }
  .column { position: relative; display: flex; flex-direction: column; min-height: 0; background: var(--bg-1); border-inline-start: 1px solid var(--line); }
  .resize { position: absolute; inset-block: 0; inset-inline-start: -3px; width: 6px; cursor: col-resize; z-index: 2; }
  .between { height: 4px; flex: none; }
  .dragging .between { height: 10px; }
  .between.hot, .col-drop.hot { background: var(--accent); opacity: 0.8; }
  .col-drop { width: 0; flex: none; }
  .dragging .col-drop { width: 10px; }
  .icon-col { position: relative; display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 6px 4px; width: 44px; border-inline-start: 1px solid var(--line); background: var(--bg-1); }
  .icon-group { display: flex; flex-direction: column; gap: 2px; border-radius: var(--radius-sm); }
  .icon-group.hot { box-shadow: 0 0 0 2px var(--accent); }
  .icon-sep { width: 24px; height: 1px; background: var(--line); margin: 4px 0; }
  .flyout { position: absolute; top: 6px; inset-inline-end: calc(100% + 6px); width: 260px; max-height: 70vh; display: flex; flex-direction: column; background: var(--bg-2); border: 1px solid var(--line-strong); border-radius: var(--radius); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45); z-index: 20; }
  .flyout header { display: flex; align-items: center; justify-content: space-between; padding: 4px 4px 4px 10px; padding-inline-start: 10px; border-bottom: 1px solid var(--line); font-weight: 600; }
  .flyout-body { overflow: auto; min-height: 120px; }
  .ghost { position: fixed; z-index: 100; display: flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: var(--radius-sm); background: var(--bg-4); border: 1px solid var(--accent); pointer-events: none; font-size: var(--fs-1); }
</style>
