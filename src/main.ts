if (import.meta.env.VITE_SPIKE) {
  import('./spike/run').then((m) => m.runSpikes());
} else {
  document.getElementById('app')!.textContent = 'لوحة';
}
