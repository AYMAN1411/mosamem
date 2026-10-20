// Single-key shortcuts (tools, colors). Modifier shortcuts belong to the native menu.
import { app } from './state.svelte';
import { TOOL_GROUPS } from './tools';
import { layer } from './actions';

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
    if (e.code === 'KeyX') [app.fg, app.bg] = [app.bg, app.fg];
    else if (e.code === 'KeyD') {
      app.fg = '#000000';
      app.bg = '#ffffff';
    }
  });
}
