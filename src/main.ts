import { mount } from 'svelte';
import './app/fonts.css';
import './app/theme.css';
import App from './app/App.svelte';

mount(App, { target: document.getElementById('app')! });

if (import.meta.env.DEV || import.meta.env.VITE_HARNESS) {
  // Handles for the debug test harness (tools/harness.mjs).
  Promise.all([import('./app/state.svelte'), import('./app/actions'), import('./app/i18n/index.svelte'), import('./engine/bridge'), import('./app/dock/layout.svelte')]).then(
    ([state, actions, i18n, bridge, dock]) => {
      Object.assign(window, { __h: { app: state.app, actions, i18n: i18n.i18n, engine: bridge.engine, dock: dock.dock } });
    },
  );
}
