<script lang="ts">
  // Mosamem mark: a canvas frame with a single brush stroke that reads as an initial meem (مـ),
  // as at the start of «مصمم»: a round head on the baseline stroke.
  let { size = 64 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
  <rect x="4" y="4" width="56" height="56" rx="14" fill="#24262c" stroke="#3a3e47" stroke-width="2" />
  <path
    d="M15 39 L37 39 C44 39 49 35.5 49 29.5 C49 23.5 44.5 20 40 20.5 C35 21 32.5 25.5 33.5 30.5 C34.3 34.5 37 37.5 40.5 38.6"
    fill="none"
    stroke="#3fc4aa"
    stroke-width="6.5"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
</svg>
