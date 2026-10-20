<script lang="ts">
  // A 256-bin histogram, dark values on the left. Scaled so a few huge bins don't flatten the rest.
  let { data, color = 'var(--fg-2)', height = 90, marks = [] }: { data: number[] | null; color?: string; height?: number; marks?: number[] } = $props();

  const path = $derived.by(() => {
    if (!data || !data.length) return '';
    const sorted = data.filter((v) => v > 0).sort((a, b) => a - b);
    const cap = sorted.length ? sorted[Math.floor(sorted.length * 0.98)] || 1 : 1;
    let d = `M0 ${height}`;
    for (let i = 0; i < 256; i++) {
      const h = Math.min(1, data[i] / cap) * (height - 2);
      d += `L${i} ${height - h}L${i + 1} ${height - h}`;
    }
    return d + `L256 ${height}Z`;
  });
</script>

<svg class="hist" viewBox={`0 0 256 ${height}`} preserveAspectRatio="none" style:height={`${height}px`}>
  <path d={path} fill={color} opacity="0.75" />
  {#each marks as m (m)}
    <line x1={m} x2={m} y1="0" y2={height} stroke="var(--accent)" stroke-width="1.2" vector-effect="non-scaling-stroke" />
  {/each}
</svg>

<style>
  .hist { display: block; width: 100%; background: var(--bg-0); border: 1px solid var(--line); border-radius: var(--radius-sm); }
</style>
