// Panel docking model. Columns sit on the "end" side of the window (right in English, left in
// Arabic) and are listed from the canvas outward. CSS logical properties do the mirroring.
import type { IconName } from '../icons/icons';
import type { Key } from '../i18n/index.svelte';

export type PanelId = 'navigator' | 'color' | 'brushes' | 'adjustments' | 'properties' | 'layers' | 'history';

export const PANELS: Record<PanelId, { title: Key; icon: IconName }> = {
  navigator: { title: 'panel.navigator', icon: 'navigator' },
  color: { title: 'panel.color', icon: 'color' },
  brushes: { title: 'panel.brushes', icon: 'brushes' },
  adjustments: { title: 'panel.adjustments', icon: 'adjustments' },
  properties: { title: 'panel.properties', icon: 'properties' },
  layers: { title: 'panel.layers', icon: 'layers' },
  history: { title: 'panel.history', icon: 'history' },
};

export interface Stack {
  id: string;
  panels: PanelId[];
  active: PanelId;
  collapsed: boolean;
  /** Share of the column's free height. */
  grow: number;
}

export interface Column {
  id: string;
  stacks: Stack[];
  width: number;
  /** Icon strip: panels open as flyouts. */
  iconic: boolean;
}

let uid = 1;
const nid = (p: string) => `${p}${Date.now().toString(36)}${uid++}`;

export function defaultLayout(): Column[] {
  return [
    {
      id: 'icons',
      width: 44,
      iconic: true,
      stacks: [{ id: 'sh', panels: ['history', 'navigator'], active: 'history', collapsed: false, grow: 1 }],
    },
    {
      id: 'main',
      width: 292,
      iconic: false,
      stacks: [
        { id: 'sc', panels: ['color', 'brushes'], active: 'color', collapsed: false, grow: 0.9 },
        { id: 'sp', panels: ['properties', 'adjustments'], active: 'properties', collapsed: false, grow: 1 },
        { id: 'sl', panels: ['layers'], active: 'layers', collapsed: false, grow: 1.6 },
      ],
    },
  ];
}

const STORE = 'mosamem.dock.v1';

function load(): Column[] {
  try {
    const raw = localStorage.getItem(STORE);
    if (raw) {
      const cols = JSON.parse(raw) as Column[];
      const seen = new Set(cols.flatMap((c) => c.stacks.flatMap((s) => s.panels)));
      if (Object.keys(PANELS).every((p) => seen.has(p as PanelId))) return cols;
    }
  } catch {
    /* fall back to default */
  }
  return defaultLayout();
}

class Dock {
  columns = $state<Column[]>(load());
  /** Panel shown as a flyout from an icon column. */
  flyout = $state<PanelId | null>(null);
  drag = $state<{ panel: PanelId; x: number; y: number; target: DropTarget | null } | null>(null);

  save() {
    try {
      localStorage.setItem(STORE, JSON.stringify(this.columns));
    } catch {
      /* storage unavailable */
    }
  }

  reset() {
    this.columns = defaultLayout();
    this.flyout = null;
    this.save();
  }

  find(panel: PanelId): { col: Column; stack: Stack } | null {
    for (const col of this.columns) for (const stack of col.stacks) if (stack.panels.includes(panel)) return { col, stack };
    return null;
  }

  /** Brings a panel to the front, opening its stack or flyout. */
  show(panel: PanelId) {
    const f = this.find(panel);
    if (!f) return;
    f.stack.active = panel;
    f.stack.collapsed = false;
    if (f.col.iconic) this.flyout = panel;
    this.save();
  }

  private detach(panel: PanelId) {
    const f = this.find(panel);
    if (!f) return;
    f.stack.panels = f.stack.panels.filter((p) => p !== panel);
    if (f.stack.active === panel) f.stack.active = f.stack.panels[0];
    if (!f.stack.panels.length) f.col.stacks = f.col.stacks.filter((s) => s !== f.stack);
    this.columns = this.columns.filter((c) => c.stacks.length);
  }

  apply(panel: PanelId, target: DropTarget) {
    if (target.kind === 'stack') {
      const dest = this.columns.flatMap((c) => c.stacks).find((s) => s.id === target.stack);
      if (!dest || dest.panels.includes(panel)) {
        if (dest) dest.active = panel;
        return;
      }
      this.detach(panel);
      dest.panels.push(panel);
      dest.active = panel;
    } else if (target.kind === 'between') {
      this.detach(panel);
      const col = this.columns.find((c) => c.id === target.column);
      if (!col) return;
      col.stacks.splice(Math.min(target.index, col.stacks.length), 0, { id: nid('s'), panels: [panel], active: panel, collapsed: false, grow: 1 });
    } else {
      this.detach(panel);
      const col: Column = { id: nid('c'), width: 260, iconic: false, stacks: [{ id: nid('s'), panels: [panel], active: panel, collapsed: false, grow: 1 }] };
      this.columns.splice(Math.min(target.index, this.columns.length), 0, col);
    }
    this.flyout = null;
    this.save();
  }
}

export type DropTarget = { kind: 'stack'; stack: string } | { kind: 'between'; column: string; index: number } | { kind: 'column'; index: number };

export const dock = new Dock();
