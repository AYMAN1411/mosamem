<script lang="ts">
  // A percentage field: type a value, drag on the label to scrub, or use the slider popover.
  // `onchange(value, live)`: live = true while dragging (no history step), false on release.
  import { fmt } from '../i18n/index.svelte';

  let { label, value, onchange }: { label: string; value: number; onchange: (v: number, live: boolean) => void } = $props();
  let open = $state(false);
  let local = $state<number | null>(null);
  const shown = $derived(local ?? value);
  let pending = false;
  let queued: number | null = null;

  // at most one live update in flight; the latest value wins
  async function live(v: number) {
    local = v;
    if (pending) {
      queued = v;
      return;
    }
    pending = true;
    await onchange(v, true);
    pending = false;
    if (queued != null) {
      const q = queued;
      queued = null;
      live(q);
    }
  }

  async function commit(v: number) {
    queued = null;
    while (pending) await new Promise((r) => setTimeout(r, 5));
    await onchange(v, false);
    local = null;
  }

  function scrubStart(e: PointerEvent) {
    const start = e.clientX;
    const v0 = value;
    const rtl = document.documentElement.dir === 'rtl';
    let last = v0;
    const m = (ev: PointerEvent) => {
      const dx = (ev.clientX - start) * (rtl ? -1 : 1);
      last = Math.max(0, Math.min(1, v0 + dx / 200));
      live(Math.round(last * 100) / 100);
    };
    window.addEventListener('pointermove', m);
    window.addEventListener('pointerup', () => {
      window.removeEventListener('pointermove', m);
      if (last !== v0) commit(Math.round(last * 100) / 100);
    }, { once: true });
  }
</script>

<span class="scrub">
  <span class="label" onpointerdown={scrubStart} role="slider" aria-valuenow={Math.round(shown * 100)} tabindex="-1">{label}</span>
  <input
    class="field num"
    value={`${fmt(shown * 100)}%`}
    onfocus={(e) => e.currentTarget.select()}
    onchange={(e) => { const n = parseFloat(e.currentTarget.value.replace(/[%٪]/, '')); if (!isNaN(n)) commit(Math.max(0, Math.min(100, n)) / 100); }}
  />
  <button class="caret" onclick={() => (open = !open)} aria-label="slider">▾</button>
  {#if open}
    <span class="pop">
      <input
        type="range"
        min="0"
        max="100"
        value={Math.round(shown * 100)}
        oninput={(e) => live(Number(e.currentTarget.value) / 100)}
        onchange={(e) => { commit(Number(e.currentTarget.value) / 100); open = false; }}
      />
    </span>
  {/if}
</span>

<style>
  .scrub { position: relative; display: inline-flex; align-items: center; gap: 4px; }
  .label { color: var(--fg-2); font-size: var(--fs-0); cursor: ew-resize; }
  input.field { width: 54px; height: 24px; padding: 0 4px; text-align: center; }
  .caret { width: 14px; padding: 0; border: 0; background: none; color: var(--fg-2); }
  .pop { position: absolute; top: 28px; inset-inline-end: 0; z-index: 30; padding: 8px 10px; background: var(--bg-2); border: 1px solid var(--line-strong); border-radius: var(--radius); box-shadow: 0 8px 20px rgba(0, 0, 0, 0.4); }
  .pop input { width: 160px; accent-color: var(--accent); }
</style>
