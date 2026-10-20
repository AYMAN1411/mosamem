// Single-key shortcuts (tools, colors). Modifier shortcuts belong to the native menu.
import { app } from './state.svelte';
import { TOOL_GROUPS } from './tools';
import { layer } from './actions';
import { settingsFor } from './canvas/paintTools';

function editable(t: EventTarget | null) {
  return t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement || (t instanceof HTMLElement && t.isContentEditable);
}

export function installShortcuts() {
  window.addEventListener('keydown', (e) => {
    if (e.metaKey || e.ctrlKey || e.altKey || editable(e.target) || app.dialog) return;
    const group = TOOL_GROUPS.find((g) => g.code === e.code);
    if (group) {
      e.preventDefault();
      const i = group.tools.findIndex((tl) => tl.id === app.tool);
      // Shift+letter cycles through the group's tools; the letter alone picks the group.
      app.tool = e.shiftKey && i >= 0 ? group.tools[(i + 1) % group.tools.length].id : i >= 0 ? app.tool : group.tools[0].id;
      return;
    }
    if ((e.code === 'Delete' || e.code === 'Backspace') && app.active) {
      e.preventDefault();
      layer.delete();
      return;
    }
    // brush size [ ], hardness Shift+[ ], opacity 1–0, flow Shift+1–0 (Photoshop conventions)
    const s = settingsFor(app.tool);
    if (s && (e.code === 'BracketLeft' || e.code === 'BracketRight')) {
      e.preventDefault();
      const up = e.code === 'BracketRight';
      if (e.shiftKey) s.hardness = Math.max(0, Math.min(1, Math.round((s.hardness + (up ? 0.25 : -0.25)) * 4) / 4));
      else {
        const step = s.diameter < 10 ? 1 : s.diameter < 50 ? 5 : s.diameter < 100 ? 10 : s.diameter < 300 ? 25 : 50;
        s.diameter = Math.max(1, Math.min(5000, s.diameter + (up ? step : -step)));
      }
      return;
    }
    const digit = /^Digit(\d)$/.exec(e.code);
    if (s && digit) {
      e.preventDefault();
      const v = digit[1] === '0' ? 1 : Number(digit[1]) / 10;
      if (e.shiftKey) s.flow = v;
      else s.opacity = v;
      return;
    }
    if (e.code === 'KeyX') [app.fg, app.bg] = [app.bg, app.fg];
    else if (e.code === 'KeyD') {
      app.fg = '#000000';
      app.bg = '#ffffff';
    }
  });
}
