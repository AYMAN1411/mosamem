<script lang="ts">
  import { app } from '../state.svelte';
  import { t } from '../i18n/index.svelte';
  import { select } from '../actions';
  import { ai, aiActions } from '../ai.svelte';
  import Icon from '../icons/Icon.svelte';

  const tool = $derived(app.tool);
  const s = $derived(app.sel);
  const modes = [
    { id: 'replace', label: 'opt.selNew', glyph: '■' },
    { id: 'add', label: 'opt.selAdd', glyph: '⊕' },
    { id: 'subtract', label: 'opt.selSub', glyph: '⊖' },
    { id: 'intersect', label: 'opt.selInt', glyph: '⊗' },
  ] as const;
</script>

<div class="seg" role="group">
  {#each modes as m (m.id)}
    <button class="icon-btn" title={t(m.label)} aria-pressed={s.mode === m.id} onclick={() => (app.sel.mode = m.id)}><span class="glyph">{m.glyph}</span></button>
  {/each}
</div>
<div class="sep"></div>

{#if tool === 'marqueeRect' || tool === 'marqueeEllipse' || tool === 'lasso' || tool === 'lassoPolygon'}
  <label class="inline">{t('opt.feather')}<input class="field num small" type="number" min="0" max="1000" step="0.5" bind:value={app.sel.feather} />px</label>
{/if}
{#if tool !== 'marqueeRect' && tool !== 'quickSelect' && tool !== 'objectSelect'}
  <label class="check"><input type="checkbox" bind:checked={app.sel.antialias} />{t('opt.antialias')}</label>
{/if}
{#if tool === 'marqueeRect' || tool === 'marqueeEllipse'}
  <label class="inline">{t('opt.style')}
    <select class="field" bind:value={app.sel.style}>
      <option value="normal">{t('opt.styleNormal')}</option>
      <option value="ratio">{t('opt.styleRatio')}</option>
      <option value="size">{t('opt.styleSize')}</option>
    </select>
  </label>
  {#if s.style === 'ratio'}
    <label class="inline">{t('opt.width')}<input class="field num small" type="number" min="0.01" step="0.01" bind:value={app.sel.ratioW} /></label>
    <label class="inline">{t('opt.height')}<input class="field num small" type="number" min="0.01" step="0.01" bind:value={app.sel.ratioH} /></label>
  {:else if s.style === 'size'}
    <label class="inline">{t('opt.width')}<input class="field num small" type="number" min="1" bind:value={app.sel.sizeW} />px</label>
    <label class="inline">{t('opt.height')}<input class="field num small" type="number" min="1" bind:value={app.sel.sizeH} />px</label>
  {/if}
{/if}
{#if tool === 'magicWand'}
  <label class="inline">{t('opt.tolerance')}<input class="field num small" type="number" min="0" max="255" bind:value={app.sel.tolerance} /></label>
  <label class="check"><input type="checkbox" bind:checked={app.sel.contiguous} />{t('opt.contiguous')}</label>
{/if}
{#if tool === 'quickSelect'}
  <label class="inline">{t('opt.brushSize')}<input class="field num small" type="number" min="1" max="2500" bind:value={app.sel.brush} />px</label>
{/if}
{#if tool === 'magicWand' || tool === 'quickSelect' || tool === 'objectSelect'}
  <label class="check"><input type="checkbox" bind:checked={app.sel.allLayers} />{t('opt.allLayers')}</label>
  <div class="sep"></div>
  <button class="btn ai" disabled={!app.active || !!ai.working} onclick={() => aiActions.selectSubject()} title={t('ai.subjectHint')}><Icon name="objectSelect" size={14} />{t('ai.selectSubject')}</button>
  <button class="btn ai" disabled={!app.active || !!ai.working} onclick={() => aiActions.removeBackground()} title={t('ai.backgroundHint')}>{t('ai.removeBackground')}</button>
{/if}
{#if tool === 'objectSelect'}
  <span class="hint">{t('ai.objectHint')}</span>
{/if}
<div class="sep"></div>
<button class="btn" disabled={!app.active?.info.hasSelection} onclick={select.refine}>{t('opt.refine')}</button>

<style>
  .seg { display: flex; gap: 1px; }
  .glyph { font-size: 15px; line-height: 1; }
  .inline { display: flex; align-items: center; gap: 6px; color: var(--fg-2); }
  .small { width: 62px; height: 24px; }
  select.field { height: 24px; }
  .check { color: var(--fg-1); }
  .ai { display: inline-flex; align-items: center; gap: 6px; }
  .hint { color: var(--fg-3); font-size: var(--fs-0); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
