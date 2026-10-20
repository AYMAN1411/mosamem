<script lang="ts">
  import { textTool } from '../canvas/text.svelte';
  import { t, type Key } from '../i18n/index.svelte';
  import Icon from '../icons/Icon.svelte';
  import { FAMILIES, type Align, type Digits, type Kashida, type TextDirection } from '../../engine/text';

  const s = $derived((void textTool.editing?.data.style, textTool.currentStyle()));
  const fam = $derived(FAMILIES.find((f) => f.id === s.family)!);
  const hex = (c: number[]) => `#${c.slice(0, 3).map((v) => v.toString(16).padStart(2, '0')).join('')}`;
  const rgba = (h: string): [number, number, number, number] => [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16), 255];
  const rtl = $derived(s.direction !== 'ltr');
  // alignment icons follow the text's direction: "start" is right-aligned in Arabic text
  const aligns: { id: Align; icon: 'alignLeft' | 'alignCenter' | 'alignRight' | 'alignJustify' }[] = $derived([
    { id: 'start', icon: rtl ? 'alignRight' : 'alignLeft' },
    { id: 'center', icon: 'alignCenter' },
    { id: 'end', icon: rtl ? 'alignLeft' : 'alignRight' },
    { id: 'justify', icon: 'alignJustify' },
  ]);
  const num = (e: Event & { currentTarget: HTMLInputElement }) => parseFloat(e.currentTarget.value.replace(/[٫,]/, '.'));
</script>

<select class="field family" value={s.family} onchange={(e) => textTool.setStyle({ family: e.currentTarget.value as never, weight: 400 })} title={t('text.family')}>
  {#each FAMILIES as f (f.id)}<option value={f.id} selected={f.id === s.family} style:font-family={`'${f.css}'`}>{f.css}</option>{/each}
</select>
<select class="field" value={s.weight} onchange={(e) => textTool.setStyle({ weight: Number(e.currentTarget.value) })} title={t('text.weight')}>
  {#each fam.weights as w (w)}<option value={w} selected={w === s.weight}>{t(`weight.${w}` as Key)}</option>{/each}
</select>
{#if fam.italic}
  <button class="icon-btn small" aria-pressed={s.italic} title={t('text.italic')} onclick={() => textTool.setStyle({ italic: !s.italic })}><i>I</i></button>
{/if}
<label class="inline" title={t('text.size')}><Icon name="textLayer" size={14} /><input class="field num" value={s.size} onchange={(e) => num(e) > 0 && textTool.setStyle({ size: num(e) })} />px</label>
<input class="swatch" type="color" value={hex(s.color)} title={t('text.color')} onchange={(e) => textTool.setStyle({ color: rgba(e.currentTarget.value) })} />
<span class="group" role="radiogroup" aria-label={t('text.align')}>
  {#each aligns as a (a.id)}
    <button class="icon-btn small" aria-pressed={s.align === a.id} title={t(`text.align.${a.id}` as Key)} onclick={() => textTool.setStyle({ align: a.id })}><Icon name={a.icon} size={15} /></button>
  {/each}
</span>
<select class="field" value={s.direction} onchange={(e) => textTool.setStyle({ direction: e.currentTarget.value as TextDirection })} title={t('text.direction')}>
  {#each ['rtl', 'ltr', 'auto'] as d (d)}<option value={d} selected={d === s.direction}>{t(`text.dir.${d}` as Key)}</option>{/each}
</select>
<label class="inline" title={t('text.lineHeight')}>↕<input class="field num" placeholder={t('text.auto')} value={s.lineHeight ?? ''} onchange={(e) => textTool.setStyle({ lineHeight: e.currentTarget.value.trim() ? num(e) : null })} /></label>
<label class="inline" title={t('text.tracking')}>↔<input class="field num" value={s.tracking} onchange={(e) => textTool.setStyle({ tracking: num(e) || 0 })} /></label>
<select class="field" value={s.digits} onchange={(e) => textTool.setStyle({ digits: e.currentTarget.value as Digits })} title={t('text.digits')}>
  {#each ['western', 'arabicIndic', 'persian'] as d (d)}<option value={d} selected={d === s.digits}>{t(`text.digits.${d}` as Key)}</option>{/each}
</select>
<select class="field" value={s.kashida} disabled={s.align !== 'justify'} onchange={(e) => textTool.setStyle({ kashida: e.currentTarget.value as Kashida })} title={t('text.kashida')}>
  {#each ['none', 'short', 'medium', 'long'] as k (k)}<option value={k} selected={k === s.kashida}>{t(`text.kashida.${k}` as Key)}</option>{/each}
</select>
{#if textTool.editing}
  <button class="icon-btn" title={t('text.cancel')} onclick={() => textTool.cancel()}><Icon name="close" size={16} /></button>
  <button class="btn primary small-btn" title={t('text.commit')} onclick={() => textTool.commit()}>✓</button>
{/if}

<style>
  .family { max-width: 170px; }
  .inline { display: flex; align-items: center; gap: 3px; color: var(--fg-2); font-size: var(--fs-0); }
  .inline input { width: 48px; height: 24px; text-align: center; }
  .group { display: inline-flex; gap: 1px; }
  .small { width: 26px; height: 26px; }
  .small-btn { height: 24px; padding: 0 10px; }
  .swatch { width: 28px; height: 24px; padding: 0; border: 1px solid var(--line-strong); border-radius: var(--radius-sm); background: none; }
  select.field { height: 26px; max-width: 120px; }
</style>
