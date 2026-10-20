import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Cross-origin isolation (COOP/COEP) enables SharedArrayBuffer and wasm threads.
const isolation = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
};

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, headers: isolation, watch: { ignored: ['**/src-tauri/**', '**/target/**'] } },
  preview: { headers: isolation },
  build: { target: 'safari16', sourcemap: true },
  // prepared up front so the dev server doesn't reload the page when the PSD worker first loads it
  optimizeDeps: { include: ['ag-psd'] },
  worker: { format: 'es' },
  test: { environment: 'happy-dom', include: ['src/**/*.test.ts'] },
});
