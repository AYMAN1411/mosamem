<script lang="ts">
  import { app } from '../state.svelte';
  import { t, fmt, type Key } from '../i18n/index.svelte';
  import { findLayer, walkLayers, type LayerInfo } from '../../engine/bridge';
  import { BLEND_MODES } from '../../engine/blend';
  import { layer as act } from '../actions';
  import Icon from '../icons/Icon.svelte';
  import LayerThumb from './LayerThumb.svelte';
  import Scrub from './Scrub.svelte';
  import { liveChange, liveCommit } from '../live';

  const doc = $derived(app.active);
  const active = $derived(doc ? findLayer(doc.info.layers, doc.info.active) : null);

  // rows in display order (top to bottom), skipping children of collapsed groups
  interface Row { l: LayerInfo; parent: number | null; depth: number; siblings: LayerInfo[] }
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const walk = (ls: LayerInfo[], parent: number | null, depth: number) => {
      for (const l of ls) {
        out.push({ l, parent, depth, siblings: ls });
        if (l.children && l.expanded) walk(l.children, l.id, depth + 1);
      }
    };
    if (doc) walk(doc.info.layers, null, 0);
    return out;
  });

  // ---- selection ----
  let anchor: number | null = null;
  function select(e: MouseEvent, id: number) {
    if (!doc) return;
    if (e.shiftKey && anchor != null) {
      const ids = rows.map((r) => r.l.id);
      const [a, b] = [ids.indexOf(anchor), ids.indexOf(id)].sort((x, y) => x - y);
      doc.selection = ids.slice(a, b + 1);
    } else if (e.metaKey || e.ctrlKey) {
      doc.selection = doc.selection.includes(id) ? doc.selection.filter((x) => x !== id) : [...doc.selection, id];
      anchor = id;
    } else {
      doc.selection = [id];
      anchor = id;
    }
    if (!doc.selection.length) doc.selection = [id];
    if (doc.info.active !== id) act.activate(id);
  }

  // ---- rename ----
  let renaming = $state<number | null>(null);
  function finishRename(id: number, value: string) {
    renaming = null;
    const l = doc && findLayer(doc.info.layers, id);
    if (l && value.trim() && value !== l.name) act.set(id, { name: value.trim() });
  }

  // ---- drag to reorder ----
  type Drop = { parent: number | null; index: number; row: number; where: 'above' | 'below' | 'into' };
  let dragging = $state<number[] | null>(null);
  let drop = $state<Drop | null>(null);
  let press: { id: number; y: number } | null = null;
  let list = $state<HTMLUListElement>();

  function down(e: PointerEvent, id: number) {
    if (e.button !== 0 || renaming) return;
    press = { id, y: e.clientY };
  }

  function move(e: PointerEvent) {
    if (!press || !doc) return;
    if (!dragging && Math.abs(e.clientY - press.y) > 4) {
      dragging = doc.selection.includes(press.id) ? [...doc.selection] : [press.id];
    }
    if (!dragging) return;
    const el = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest<HTMLElement>('[data-row]');
    if (!el) {
      drop = null;
      return;
    }
    const i = Number(el.dataset.row);
    const r = rows[i];
    const box = el.getBoundingClientRect();
    const f = (e.clientY - box.top) / box.height;
    const engineIndex = r.siblings.length - 1 - r.siblings.indexOf(r.l); // siblings are top→bottom
    if (r.l.kind === 'group' && f > 0.3 && f < 0.7) drop = { parent: r.l.id, index: r.l.children?.length ?? 0, row: i, where: 'into' };
    else if (f <= 0.5) drop = { parent: r.parent, index: engineIndex + 1, row: i, where: 'above' };
    else drop = { parent: r.parent, index: engineIndex, row: i, where: 'below' };
    // never into a dragged group or its descendants
    if (drop && dragging.some((id) => id === drop!.parent || isInside(drop!.parent, id))) drop = null;
  }

  function isInside(id: number | null, ancestor: number): boolean {
    if (id == null || !doc) return false;
    const a = findLayer(doc.info.layers, ancestor);
    let found = false;
    if (a?.children) walkLayers(a.children, (l) => (found ||= l.id === id));
    return found;
  }

  function up() {
    if (dragging && drop) act.move(dragging, drop.parent, drop.index);
    dragging = null;
    drop = null;
    press = null;
  }

  // ---- header controls ----
  function blendValue(l: LayerInfo) {
    return l.kind === 'group' && l.passThrough ? 'passThrough' : l.blend;
  }
  async function setBlend(v: string) {
    if (!active) return;
    if (v === 'passThrough') return act.set(active.id, { passThrough: true });
    // preview on the GPU at once; the engine's composite replaces it when ready
    await liveChange(active.id, { blend: v as LayerInfo['blend'] });
    await liveCommit(active.id, { blend: v as LayerInfo['blend'] });
  }
  function setLock(key: 'transparent' | 'pixels' | 'position' | 'all') {
    if (!active) return;
    const l = active.locks;
    const all = l.transparent && l.pixels && l.position;
    const next = key === 'all' ? { transparent: !all, pixels: !all, position: !all } : { ...l, [key]: !l[key] };
    act.set(active.id, { locks: next });
  }
  const groups: string[][] = [['normal', 'dissolve'], ['darken', 'multiply', 'colorBurn', 'linearBurn', 'darkerColor'], ['lighten', 'screen', 'colorDodge', 'linearDodge', 'lighterColor'], ['overlay', 'softLight', 'hardLight', 'vividLight', 'linearLight', 'pinLight', 'hardMix'], ['difference', 'exclusion', 'subtract', 'divide'], ['hue', 'saturation', 'color', 'luminosity']];
  void BLEND_MODES;
</script>

<svelte:window onpointermove={move} onpointerup={up} />

{#if doc && active}
  <div class="panel">
    <div class="controls">
      <select class="field blend" value={blendValue(active)} onchange={(e) => setBlend(e.currentTarget.value)} title={t('layer.blend')}>
        {#if active.kind === 'group'}<option value="passThrough">{t('blend.passThrough')}</option>{/if}
        {#each groups as g, gi (gi)}
          <optgroup label="―――">
            {#each g as m (m)}<option value={m}>{t(`blend.${m}` as Key)}</option>{/each}
          </optgroup>
        {/each}
      </select>
      <Scrub label={t('layer.opacity')} value={active.opacity} onchange={(v, live) => (live ? liveChange(active.id, { opacity: v }) : liveCommit(active.id, { opacity: v }))} />
    </div>
    <div class="controls">
      <span class="locks">
        <span class="lbl">{t('layer.lock')}</span>
        <button class="icon-btn small" title={t('layer.lockTransparent')} aria-pressed={active.locks.transparent} onclick={() => setLock('transparent')}><Icon name="marqueeRect" size={14} /></button>
        <button class="icon-btn small" title={t('layer.lockPixels')} aria-pressed={active.locks.pixels} onclick={() => setLock('pixels')}><Icon name="brush" size={14} /></button>
        <button class="icon-btn small" title={t('layer.lockPosition')} aria-pressed={active.locks.position} onclick={() => setLock('position')}><Icon name="move" size={14} /></button>
        <button class="icon-btn small" title={t('layer.lockAll')} aria-pressed={active.locks.transparent && active.locks.pixels && active.locks.position} onclick={() => setLock('all')}><Icon name="lock" size={14} /></button>
      </span>
      {#if active.kind === 'pixel'}
        <Scrub label={t('layer.fill')} value={active.fill} onchange={(v, live) => (live ? liveChange(active.id, { fill: v }) : liveCommit(active.id, { fill: v }))} />
      {/if}
    </div>

    <ul class="rows" bind:this={list} role="listbox" aria-multiselectable="true" class:dragging={!!dragging}>
      {#each rows as r, i (r.l.id)}
        {@const l = r.l}
        {@const sel = doc.selection.includes(l.id)}
        <li
          class="row"
          class:selected={sel}
          class:active={l.id === doc.info.active}
          class:hidden={!l.visible}
          class:drop-above={drop?.row === i && drop.where === 'above'}
          class:drop-below={drop?.row === i && drop.where === 'below'}
          class:drop-into={drop?.row === i && drop.where === 'into'}
          data-row={i}
          role="option"
          aria-selected={sel}
          onpointerdown={(e) => down(e, l.id)}
          onclick={(e) => select(e, l.id)}
          onkeydown={(e) => e.key === 'Enter' && (renaming = l.id)}
          tabindex="-1"
          ondblclick={() => (renaming = l.id)}
          style:padding-inline-start={`${6 + r.depth * 16}px`}
        >
          <button
            class="eye"
            title={`${t('layer.visible')} — ${t('layer.solo')}`}
            onclick={(e) => { e.stopPropagation(); if (e.altKey) act.solo(l.id); else act.set(l.id, { visible: !l.visible }); }}
          ><Icon name={l.visible ? 'eye' : 'eyeOff'} size={15} /></button>
          {#if l.kind === 'group'}
            <button class="disclosure" class:open={l.expanded} onclick={(e) => { e.stopPropagation(); act.set(l.id, { expanded: !l.expanded }); }} aria-label="expand">
              <Icon name="chevronDown" size={12} />
            </button>
          {/if}
          {#if l.clipped}<span class="clip" title={t('layer.clipped')}>↳</span>{/if}
          {#if l.kind === 'group'}
            <span class="folder"><Icon name="folder" size={22} /></span>
          {:else}
            <LayerThumb doc={doc.info.id} layer={l.id} thumbKey={l.thumb} />
          {/if}
          {#if renaming === l.id}
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="field rename"
              value={l.name}
              autofocus
              onclick={(e) => e.stopPropagation()}
              onkeydown={(e) => { if (e.key === 'Enter') finishRename(l.id, e.currentTarget.value); if (e.key === 'Escape') renaming = null; }}
              onblur={(e) => finishRename(l.id, e.currentTarget.value)}
            />
          {:else}
            <span class="name" class:clipped-name={l.clipped}>{l.name}</span>
          {/if}
          {#if l.locks.transparent || l.locks.pixels || l.locks.position}<span class="lock"><Icon name="lock" size={12} /></span>{/if}
          {#if l.kind === 'group' && !l.passThrough}<span class="mode">{t(`blend.${l.blend}` as Key)}</span>{/if}
          {#if l.opacity < 1}<span class="op num">{fmt(l.opacity * 100)}%</span>{/if}
        </li>
      {/each}
    </ul>

    <div class="bar">
      <button class="icon-btn" title={t('layer.newGroup')} onclick={act.newGroup}><Icon name="folder" size={16} /></button>
      <button class="icon-btn" title={t('layer.new')} onclick={act.new}><Icon name="plus" size={16} /></button>
      <button class="icon-btn" title={t('layer.delete')} onclick={act.delete}><Icon name="trash" size={16} /></button>
    </div>
  </div>
{:else}
  <div class="empty">{t('panel.empty')}</div>
{/if}

<style>
  .panel { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .controls { display: flex; align-items: center; gap: 6px; padding: 6px 8px 0; }
  .blend { flex: 1; min-width: 0; }
  .locks { display: flex; align-items: center; gap: 1px; flex: 1; }
  .lbl { color: var(--fg-2); font-size: var(--fs-0); margin-inline-end: 4px; }
  .small { width: 24px; height: 24px; }
  .rows { list-style: none; margin: 6px 0 0; padding: 0 4px 6px; flex: 1; overflow-y: auto; border-top: 1px solid var(--line); }
  .row { position: relative; display: flex; align-items: center; gap: 6px; height: 40px; padding-inline-end: 8px; border-radius: var(--radius-sm); color: var(--fg-1); cursor: default; }
  .row:hover { background: var(--bg-3); }
  .row.selected { background: var(--bg-4); color: var(--fg-0); }
  .row.active { box-shadow: inset 2px 0 0 var(--accent); }
  :global([dir='rtl']) .row.active { box-shadow: inset -2px 0 0 var(--accent); }
  .row.hidden .name, .row.hidden :global(.thumb) { opacity: 0.45; }
  .row.drop-above::before, .row.drop-below::after { content: ''; position: absolute; inset-inline: 4px; height: 2px; background: var(--accent); }
  .row.drop-above::before { top: -1px; }
  .row.drop-below::after { bottom: -1px; }
  .row.drop-into { box-shadow: inset 0 0 0 2px var(--accent); }
  .eye, .disclosure { display: grid; place-items: center; width: 22px; height: 22px; padding: 0; border: 0; background: none; color: var(--fg-2); flex: none; }
  .eye:hover { color: var(--fg-0); }
  .disclosure { width: 14px; transform: rotate(-90deg); transition: transform 0.12s; }
  :global([dir='rtl']) .disclosure { transform: rotate(90deg); }
  .disclosure.open, :global([dir='rtl']) .disclosure.open { transform: none; }
  .clip { color: var(--fg-2); font-size: 14px; }
  .folder { color: var(--fg-2); display: grid; width: 34px; place-items: center; }
  .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .clipped-name { text-decoration: underline dotted var(--fg-3); }
  .rename { flex: 1; height: 24px; }
  .lock { color: var(--fg-2); display: grid; }
  .mode, .op { color: var(--fg-3); font-size: var(--fs-0); }
  .bar { display: flex; justify-content: flex-end; gap: 2px; padding: 4px 6px; border-top: 1px solid var(--line); }
  .empty { padding: 16px; color: var(--fg-3); }
  .dragging { cursor: grabbing; }
</style>
