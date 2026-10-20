// The keyboard reference (Help › Keyboard Shortcuts). Tool keys come from the tool list itself;
// menu shortcuts are listed here and src/tests/shortcuts.test.ts checks each against menu.ts.
import { TOOL_GROUPS } from './tools';
import type { Key } from './i18n/index.svelte';

export interface Shortcut {
  /** Keys as the menu writes them ('CmdOrCtrl+Shift+S'), or a plain description key. */
  keys: string;
  /** What it does: an i18n key. */
  label: Key;
}

export interface Section {
  title: Key;
  items: Shortcut[];
}

/** Menu shortcuts; each must match an accelerator in menu.ts (checked by a test). */
export const MENU_SHORTCUTS: Section[] = [
  {
    title: 'menu.file',
    items: [
      { keys: 'CmdOrCtrl+N', label: 'file.new' },
      { keys: 'CmdOrCtrl+O', label: 'file.open' },
      { keys: 'CmdOrCtrl+S', label: 'file.save' },
      { keys: 'CmdOrCtrl+Shift+S', label: 'file.saveAs' },
      { keys: 'CmdOrCtrl+Alt+Shift+W', label: 'file.exportAs' },
      { keys: 'CmdOrCtrl+W', label: 'file.close' },
    ],
  },
  {
    title: 'menu.edit',
    items: [
      { keys: 'CmdOrCtrl+Z', label: 'edit.undo' },
      { keys: 'CmdOrCtrl+Shift+Z', label: 'edit.redo' },
      { keys: 'CmdOrCtrl+T', label: 'xf.free' },
      { keys: 'Shift+Backspace', label: 'ai.smartFillMenu' },
    ],
  },
  {
    title: 'menu.view',
    items: [
      { keys: 'CmdOrCtrl+=', label: 'view.zoomIn' },
      { keys: 'CmdOrCtrl+-', label: 'view.zoomOut' },
      { keys: 'CmdOrCtrl+0', label: 'view.fit' },
      { keys: 'CmdOrCtrl+1', label: 'view.actual' },
      { keys: 'CmdOrCtrl+R', label: 'view.rulers' },
      { keys: "CmdOrCtrl+'", label: 'view.grid' },
      { keys: 'CmdOrCtrl+;', label: 'view.guides' },
    ],
  },
  {
    title: 'menu.layer',
    items: [
      { keys: 'CmdOrCtrl+Shift+N', label: 'layer.new' },
      { keys: 'CmdOrCtrl+J', label: 'layer.duplicate' },
      { keys: 'CmdOrCtrl+G', label: 'layer.group' },
      { keys: 'CmdOrCtrl+Shift+G', label: 'layer.ungroup' },
      { keys: 'CmdOrCtrl+Alt+G', label: 'layer.clip' },
      { keys: 'CmdOrCtrl+E', label: 'layer.mergeDown' },
      { keys: 'CmdOrCtrl+Shift+E', label: 'layer.mergeVisible' },
      { keys: 'CmdOrCtrl+I', label: 'mask.invert' },
    ],
  },
  {
    title: 'menu.select',
    items: [
      { keys: 'CmdOrCtrl+A', label: 'select.all' },
      { keys: 'CmdOrCtrl+D', label: 'select.deselect' },
      { keys: 'CmdOrCtrl+Shift+D', label: 'select.reselect' },
      { keys: 'CmdOrCtrl+Shift+I', label: 'select.inverse' },
      { keys: 'Shift+F6', label: 'select.feather' },
      { keys: 'CmdOrCtrl+Alt+R', label: 'select.refine' },
    ],
  },
  {
    title: 'menu.image',
    items: [
      { keys: 'CmdOrCtrl+Alt+I', label: 'image.size' },
      { keys: 'CmdOrCtrl+Alt+C', label: 'image.canvas' },
    ],
  },
];

/** Single keys handled in shortcuts.ts and the canvas. */
export const KEY_SHORTCUTS: Shortcut[] = [
  { keys: '[  ]', label: 'keys.brushSize' },
  { keys: 'Shift+[  Shift+]', label: 'keys.hardness' },
  { keys: '1 … 0', label: 'keys.opacity' },
  { keys: 'Shift+1 … 0', label: 'keys.flow' },
  { keys: 'X', label: 'keys.swap' },
  { keys: 'D', label: 'keys.defaults' },
  { keys: 'Space', label: 'keys.pan' },
  { keys: 'Delete', label: 'keys.deleteLayer' },
  { keys: 'Shift+letter', label: 'keys.cycle' },
];

export function toolShortcuts(): Shortcut[] {
  return TOOL_GROUPS.flatMap((g) => g.tools.map((tl) => ({ keys: g.letter, label: tl.title as Key })));
}

/** 'CmdOrCtrl+Shift+S' as this platform writes it. */
export function showKeys(keys: string, mac: boolean): string {
  if (!keys.includes('+') || keys.startsWith('Shift+letter')) return keys;
  const parts = keys.split('+');
  const name = (p: string) =>
    mac
      ? ({ CmdOrCtrl: '⌘', Shift: '⇧', Alt: '⌥', Backspace: '⌫' } as Record<string, string>)[p] ?? p
      : ({ CmdOrCtrl: 'Ctrl', Backspace: 'Backspace' } as Record<string, string>)[p] ?? p;
  return mac ? parts.map(name).join('') : parts.map(name).join('+');
}
