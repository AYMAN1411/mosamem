import { describe, expect, it } from 'vitest';
import { dock, PANELS, type PanelId } from '../app/dock/layout.svelte';

const all = () => dock.columns.flatMap((c) => c.stacks.flatMap((s) => s.panels)).sort();

describe('dock layout', () => {
  it('starts with every panel exactly once', () => {
    dock.reset();
    expect(all()).toEqual((Object.keys(PANELS) as PanelId[]).sort());
  });

  it('moving a panel into another stack keeps every panel exactly once', () => {
    dock.reset();
    const layers = dock.find('layers')!.stack;
    dock.apply('color', { kind: 'stack', stack: layers.id });
    expect(dock.find('color')!.stack.id).toBe(layers.id);
    expect(layers.active).toBe('color');
    expect(all()).toEqual((Object.keys(PANELS) as PanelId[]).sort());
  });

  it('dragging out the last panel of a stack removes the empty stack, and a new column can be made', () => {
    dock.reset();
    const before = dock.columns.length;
    dock.apply('layers', { kind: 'column', index: 0 });
    expect(dock.columns.length).toBe(before + 1);
    expect(dock.columns[0].stacks[0].panels).toEqual(['layers']);
    expect(dock.columns.some((c) => c.stacks.some((s) => s.panels.length === 0))).toBe(false);
  });

  it('show() opens a collapsed stack and selects the panel', () => {
    dock.reset();
    const s = dock.find('adjustments')!.stack;
    s.collapsed = true;
    dock.show('adjustments');
    expect(s.collapsed).toBe(false);
    expect(s.active).toBe('adjustments');
  });
});
