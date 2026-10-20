import { defineConfig } from 'vite';
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
});
