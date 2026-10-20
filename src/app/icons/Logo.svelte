<script lang="ts">
  // Lawha mark: a canvas frame with a single brush stroke that reads as the letter lam (ل).
  let { size = 64 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
  <rect x="4" y="4" width="56" height="56" rx="14" fill="#24262c" stroke="#3a3e47" stroke-width="2" />
  <path
    d="M40 13 C40 25 41 33 41 39 C41 48 35 52 28 52 C20 52 15.5 46 16.5 36"
    fill="none"
    stroke="#3fc4aa"
    stroke-width="6.5"
    stroke-linecap="round"
  />
</svg>
