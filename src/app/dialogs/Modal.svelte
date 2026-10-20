<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from '../icons/Icon.svelte';

  let { title, onclose, children, footer, width = 520 }: { title: string; onclose: () => void; children: Snippet; footer?: Snippet; width?: number } = $props();
  let box = $state<HTMLDivElement>();

  $effect(() => {
    box?.querySelector<HTMLElement>('input, select, button.primary')?.focus();
  });
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="scrim" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal" role="dialog" aria-modal="true" aria-label={title} bind:this={box} style:width={`${width}px`}>
    <header>
      <h2>{title}</h2>
      <button class="icon-btn" onclick={onclose} aria-label="close"><Icon name="close" size={14} /></button>
    </header>
    <div class="content">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 80; display: grid; place-items: center; background: rgba(8, 9, 12, 0.55); }
  .modal { max-width: calc(100vw - 48px); max-height: calc(100vh - 48px); display: flex; flex-direction: column; background: var(--bg-2); border: 1px solid var(--line-strong); border-radius: 10px; box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5); }
  header { display: flex; align-items: center; justify-content: space-between; padding: 12px 12px 10px 18px; padding-inline-start: 18px; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: var(--fs-3); font-weight: 600; }
  .content { padding: 16px 18px; overflow: auto; }
  footer { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 18px; border-top: 1px solid var(--line); }
</style>
