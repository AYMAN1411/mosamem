// Tool groups in toolbar order. `code` is the physical key (KeyboardEvent.code), so shortcuts work
// the same with an Arabic keyboard layout.
import type { ToolId } from './state.svelte';
import type { IconName } from './icons/icons';
import type { Key } from './i18n/index.svelte';

export interface ToolDef {
  id: ToolId;
  icon: IconName;
  title: Key;
  /** Milestone in which the tool becomes functional (docs/plan.md); 1 = now. */
  milestone: number;
}

export interface ToolGroup {
  code: string;
  letter: string;
  tools: ToolDef[];
}

export const TOOL_GROUPS: ToolGroup[] = [
  { code: 'KeyV', letter: 'V', tools: [{ id: 'move', icon: 'move', title: 'tool.move', milestone: 2 }] },
  { code: 'KeyM', letter: 'M', tools: [
    { id: 'marqueeRect', icon: 'marqueeRect', title: 'tool.marqueeRect', milestone: 3 },
    { id: 'marqueeEllipse', icon: 'marqueeEllipse', title: 'tool.marqueeEllipse', milestone: 3 },
  ] },
  { code: 'KeyL', letter: 'L', tools: [
    { id: 'lasso', icon: 'lasso', title: 'tool.lasso', milestone: 3 },
    { id: 'lassoPolygon', icon: 'lassoPolygon', title: 'tool.lassoPolygon', milestone: 3 },
  ] },
  { code: 'KeyW', letter: 'W', tools: [
    { id: 'quickSelect', icon: 'quickSelect', title: 'tool.quickSelect', milestone: 3 },
    { id: 'magicWand', icon: 'magicWand', title: 'tool.magicWand', milestone: 3 },
  ] },
  { code: 'KeyC', letter: 'C', tools: [{ id: 'crop', icon: 'crop', title: 'tool.crop', milestone: 6 }] },
  { code: 'KeyI', letter: 'I', tools: [{ id: 'eyedropper', icon: 'eyedropper', title: 'tool.eyedropper', milestone: 4 }] },
  { code: 'KeyJ', letter: 'J', tools: [{ id: 'heal', icon: 'heal', title: 'tool.heal', milestone: 4 }] },
  { code: 'KeyB', letter: 'B', tools: [{ id: 'brush', icon: 'brush', title: 'tool.brush', milestone: 4 }] },
  { code: 'KeyS', letter: 'S', tools: [{ id: 'clone', icon: 'clone', title: 'tool.clone', milestone: 4 }] },
  { code: 'KeyE', letter: 'E', tools: [{ id: 'eraser', icon: 'eraser', title: 'tool.eraser', milestone: 4 }] },
  { code: 'KeyG', letter: 'G', tools: [
    { id: 'gradient', icon: 'gradient', title: 'tool.gradient', milestone: 4 },
    { id: 'bucket', icon: 'bucket', title: 'tool.bucket', milestone: 4 },
  ] },
  { code: 'KeyT', letter: 'T', tools: [{ id: 'text', icon: 'text', title: 'tool.text', milestone: 7 }] },
  { code: 'KeyH', letter: 'H', tools: [{ id: 'hand', icon: 'hand', title: 'tool.hand', milestone: 1 }] },
  { code: 'KeyR', letter: 'R', tools: [{ id: 'rotateView', icon: 'rotateView', title: 'tool.rotateView', milestone: 1 }] },
  { code: 'KeyZ', letter: 'Z', tools: [{ id: 'zoom', icon: 'zoom', title: 'tool.zoom', milestone: 1 }] },
];

export const TOOLS: Record<ToolId, ToolDef> = Object.fromEntries(TOOL_GROUPS.flatMap((g) => g.tools.map((tl) => [tl.id, tl]))) as Record<ToolId, ToolDef>;

export function groupOf(id: ToolId): ToolGroup {
  return TOOL_GROUPS.find((g) => g.tools.some((tl) => tl.id === id))!;
}
