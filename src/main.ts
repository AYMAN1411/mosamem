import { mount } from 'svelte';
import './app/fonts.css';
import './app/theme.css';
import App from './app/App.svelte';

mount(App, { target: document.getElementById('app')! });

if (import.meta.env.DEV || import.meta.env.VITE_HARNESS) {
  // Handles for the debug test harness (tools/harness.mjs).
  Promise.all([import('./app/state.svelte'), import('./app/actions'), import('./app/i18n/index.svelte'), import('./engine/bridge'), import('./app/dock/layout.svelte'), import('./engine/display'), import('./app/live'), import('./app/canvas/transform.svelte'), import('./app/canvas/crop.svelte')]).then(
    ([state, actions, i18n, bridge, dock, display, live, transform, crop]) => {
      Object.assign(window, { __h: { app: state.app, actions, i18n: i18n.i18n, t: i18n.t, engine: bridge.engine, bridge, dock: dock.dock, display: display.display, live, transform: transform.transform, crop: crop.crop } });
    },
  );
}
