<script lang="ts">
  // The text being typed, over the canvas: the bundled font at the layer's size, laid out in the
  // layer's own px and mapped to the screen by the layer's placement and the view.
  import { onMount } from 'svelte';
  import { textTool } from './text.svelte';
  import { app } from '../state.svelte';
  import { FAMILIES } from '../../engine/text';

  let el = $state<HTMLDivElement>();
  const ed = $derived(textTool.editing);
  const d = $derived(app.active);

  // layer px → document px → viewport CSS px, as one CSS matrix
  const css = $derived.by(() => {
    if (!ed || !d) return '';
    const v = d.view;
    const r = (v.rotation * Math.PI) / 180;
    const z = v.zoom;
    const [cos, sin] = [Math.cos(r), Math.sin(r)];
    const V = [z * cos, -z * sin, app.viewport.w / 2 - z * (cos * v.cx - sin * v.cy), z * sin, z * cos, app.viewport.h / 2 - z * (sin * v.cx + cos * v.cy)];
    const M = ed.data.matrix;
    const a = V[0] * M[0] + V[1] * M[3];
    const c = V[0] * M[1] + V[1] * M[4];
    const e = V[0] * M[2] + V[1] * M[5] + V[2];
    const b = V[3] * M[0] + V[4] * M[3];
    const dd = V[3] * M[1] + V[4] * M[4];
    const f = V[3] * M[2] + V[4] * M[5] + V[5];
    return `matrix(${a}, ${b}, ${c}, ${dd}, ${e}, ${f})`;
  });

  const s = $derived(ed?.data.style);
  const lineHeight = $derived(s ? (s.lineHeight ?? s.size * 1.2) : 0);
  // CSS puts the first baseline (L − ascent − descent)/2 + ascent below the top of the line
  const [asc, desc] = $derived(textTool.metrics);
  const halfLeading = $derived((lineHeight - asc - desc) / 2);
  const box = $derived(ed?.data.width != null);
  const rtl = $derived(s ? s.direction !== 'ltr' : true);
  const place = $derived.by(() => {
    if (!s) return '';
    if (box) return `left: 0; top: ${-halfLeading}px; width: ${ed!.data.width}px; white-space: pre-wrap;`;
    const top = `top: ${-(halfLeading + asc)}px; white-space: pre;`;
    const align = s.align === 'justify' ? 'start' : s.align;
    if (align === 'center') return `${top} left: 0; transform: translateX(-50%);`;
    const right = (align === 'start') === rtl;
    return `${top} ${right ? 'right: 0' : 'left: 0'};`;
  });

  onMount(() => {
    if (!el) return;
    el.innerText = ed?.data.text ?? '';
    el.focus();
    // caret at the end
    const range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(false);
    const sel = getSelection();
    sel?.removeAllRanges();
    sel?.addRange(range);
  });

  function keydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') {
      e.preventDefault();
      void textTool.cancel();
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void textTool.commit();
    }
  }
</script>

{#if ed && s && d && ed.doc === d.info.id}
  <div class="frame" style:transform={css}>
    <div
      bind:this={el}
      class="editor"
      contenteditable="plaintext-only"
      role="textbox"
      tabindex="0"
      aria-multiline="true"
      spellcheck="false"
      dir={s.direction === 'auto' ? 'auto' : s.direction}
      style={place}
      style:font-family={`'${FAMILIES.find((f) => f.id === s.family)?.css}'`}
      style:font-weight={s.weight}
      style:font-style={s.italic ? 'italic' : 'normal'}
      style:font-size={`${s.size}px`}
      style:line-height={`${lineHeight}px`}
      style:letter-spacing={`${s.tracking / 1000}em`}
      style:color={`rgba(${s.color[0]}, ${s.color[1]}, ${s.color[2]}, ${s.color[3] / 255})`}
      style:text-align={s.align === 'justify' ? 'justify' : s.align}
      oninput={() => textTool.setText(el!.innerText.replace(/\n$/, ''))}
      onkeydown={keydown}
      onpointerdown={(e) => e.stopPropagation()}
    ></div>
  </div>
{/if}

<style>
  .frame { position: absolute; left: 0; top: 0; width: 0; height: 0; transform-origin: 0 0; z-index: 5; }
  .editor { position: absolute; min-width: 2px; outline: none; caret-color: var(--accent); cursor: text; font-kerning: normal; font-feature-settings: 'kern'; }
</style>
