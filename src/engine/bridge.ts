// Typed calls into the native engine.
import { invoke } from '@tauri-apps/api/core';
import type { BlendMode } from './blend';

export interface Locks {
  transparent: boolean;
  pixels: boolean;
  position: boolean;
}

export interface LayerInfo {
  id: number;
  name: string;
  kind: 'pixel' | 'group';
  visible: boolean;
  opacity: number;
  fill: number;
  blend: BlendMode;
  passThrough: boolean;
  expanded: boolean;
  locks: Locks;
  clipped: boolean;
  mask: { enabled: boolean; density: number; feather: number; thumb: number } | null;
  /** Changes when the layer's pixels change (thumbnail cache key). */
  thumb: number;
  /** Top to bottom. */
  children?: LayerInfo[];
}

export interface HistoryLabel {
  key: string;
  arg: string | null;
}

export interface DocInfo {
  id: number;
  name: string;
  path: string | null;
  width: number;
  height: number;
  ppi: number;
  /** Top to bottom. */
  layers: LayerInfo[];
  active: number;
  version: number;
  dirty: boolean;
  history: { labels: HistoryLabel[]; current: number };
  hasSelection: boolean;
  hasLastSelection: boolean;
  refining: boolean;
  /** Changes only when the selection changes; 0 when nothing is selected. */
  selectionKey: number;
}

export type Combine = 'replace' | 'add' | 'subtract' | 'intersect';
export type Shape =
  | { kind: 'rect'; x0: number; y0: number; x1: number; y1: number }
  | { kind: 'ellipse'; x0: number; y0: number; x1: number; y1: number; antialias: boolean }
  | { kind: 'polygon'; points: [number, number][]; antialias: boolean };
export type Modify = { op: 'feather' | 'expand' | 'contract' | 'border'; amount: number } | { op: 'smooth'; amount: number };
export interface RefineParams {
  radius: number;
  smooth: number;
  feather: number;
  contrast: number;
  shift: number;
}
export type RefineOutput = 'selection' | 'layerMask' | 'newLayer' | 'newLayerWithMask';
export type NewMask = 'revealAll' | 'hideAll' | 'revealSelection' | 'hideSelection';

export interface LayerProps {
  name?: string;
  visible?: boolean;
  opacity?: number;
  fill?: number;
  blend?: BlendMode;
  passThrough?: boolean;
  locks?: Locks;
  clipped?: boolean;
  expanded?: boolean;
}

export interface RecoveredFile {
  file: string;
  name: string;
  originalPath: string | null;
  savedAt: number;
  width: number;
  height: number;
}

export type Background = { kind: 'white' } | { kind: 'black' } | { kind: 'transparent' } | { kind: 'color'; rgba: [number, number, number, number] };

export type ExportFormat = 'png' | 'jpeg' | 'webp' | 'tiff' | 'heic';

const isWindows = navigator.userAgent.includes('Windows');
const base = isWindows ? 'http://lawha.localhost' : 'lawha://localhost';

/** URL of one flattened tile. Windows WebView2 serves custom schemes as http://<scheme>.localhost. */
export function tileUrl(doc: number, version: number, lod: number, tx: number, ty: number): string {
  return `${base}/composite/${doc}/${version}/${lod}/${tx}/${ty}`;
}

/** URL of one tile of a live-session part. */
export function liveUrl(session: number, part: 'below' | 'layer' | 'above', lod: number, tx: number, ty: number): string {
  return `${base}/live/${session}/${part}/${lod}/${tx}/${ty}`;
}

/** URL of one selection tile (coverage in all four channels). */
export function selUrl(doc: number, key: number, lod: number, tx: number, ty: number): string {
  return `${base}/sel/${doc}/${key}/${lod}/${tx}/${ty}`;
}

/** URL of a layer-mask thumbnail. */
export function maskThumbUrl(doc: number, layer: number, key: number, size: number): string {
  return `${base}/mthumb/${doc}/${layer}/${key}/${size}`;
}

/** URL of a layer thumbnail: `[w u16][h u16][RGBA…]`. */
export function thumbUrl(doc: number, layer: number, key: number, size: number): string {
  return `${base}/thumb/${doc}/${layer}/${key}/${size}`;
}

export const engine = {
  newDocument: (spec: { name: string; width: number; height: number; ppi: number; background: Background; backgroundLayerName: string }) =>
    invoke<DocInfo>('new_document', { spec }),
  openFile: (path: string, layerName: string) => invoke<DocInfo>('open_file', { path, layerName }),
  saveDocument: (doc: number, path: string) => invoke<DocInfo>('save_document', { doc, path }),
  exportImage: (spec: { doc: number; path: string; format?: ExportFormat; quality: number; lossless: boolean }) => invoke<void>('export_image', { spec }),
  closeDocument: (doc: number) => invoke<void>('close_document', { doc }),
  documentInfo: (doc: number) => invoke<DocInfo>('document_info', { doc }),

  layerNew: (doc: number, name: string) => invoke<DocInfo>('layer_new', { doc, name }),
  layerGroup: (doc: number, name: string, ids: number[]) => invoke<DocInfo>('layer_group', { doc, name, ids }),
  layerUngroup: (doc: number, id: number) => invoke<DocInfo>('layer_ungroup', { doc, id }),
  layerDuplicate: (doc: number, ids: number[], suffix: string) => invoke<DocInfo>('layer_duplicate', { doc, ids, suffix }),
  layerDelete: (doc: number, ids: number[]) => invoke<DocInfo>('layer_delete', { doc, ids }),
  layerSet: (doc: number, id: number, props: LayerProps, transient = false) => invoke<DocInfo>('layer_set', { doc, id, props, transient }),
  layerSolo: (doc: number, id: number) => invoke<DocInfo>('layer_solo', { doc, id }),
  layerMove: (doc: number, ids: number[], parent: number | null, index: number) => invoke<DocInfo>('layer_move', { doc, ids, parent, index }),
  layerMergeDown: (doc: number, id: number) => invoke<DocInfo>('layer_merge_down', { doc, id }),
  layerMergeVisible: (doc: number, name: string) => invoke<DocInfo>('layer_merge_visible', { doc, name }),
  layerFlatten: (doc: number, name: string) => invoke<DocInfo>('layer_flatten', { doc, name }),
  setActive: (doc: number, id: number) => invoke<DocInfo>('set_active', { doc, id }),

  selectShape: (doc: number, shape: Shape, mode: Combine, feather: number) => invoke<DocInfo>('select_shape', { doc, shape, mode, feather }),
  selectWand: (doc: number, spec: { x: number; y: number; tolerance: number; contiguous: boolean; allLayers: boolean; antialias: boolean; mode: Combine }) =>
    invoke<DocInfo>('select_wand', { doc, spec }),
  selectQuick: (doc: number, points: [number, number][], radius: number, allLayers: boolean, subtract: boolean) =>
    invoke<DocInfo>('select_quick', { doc, points, radius, allLayers, subtract }),
  selectSimple: (doc: number, op: 'all' | 'deselect' | 'reselect' | 'invert') => invoke<DocInfo>('select_simple', { doc, op }),
  selectModify: (doc: number, op: Modify) => invoke<DocInfo>('select_modify', { doc, op }),
  selectTranslate: (doc: number, dx: number, dy: number) => invoke<DocInfo>('select_translate', { doc, dx, dy }),
  selectHit: (doc: number, x: number, y: number) => invoke<boolean>('select_hit', { doc, x, y }),
  refinePreview: (doc: number, params: RefineParams) => invoke<DocInfo>('refine_preview', { doc, params }),
  refineCancel: (doc: number) => invoke<DocInfo>('refine_cancel', { doc }),
  refineApply: (doc: number, params: RefineParams, output: RefineOutput, decontaminate: boolean, layerName: string) =>
    invoke<DocInfo>('refine_apply', { doc, params, output, decontaminate, layerName }),
  maskAdd: (doc: number, id: number, kind: NewMask) => invoke<DocInfo>('mask_add', { doc, id, kind }),
  maskDelete: (doc: number, id: number) => invoke<DocInfo>('mask_delete', { doc, id }),
  maskApply: (doc: number, id: number) => invoke<DocInfo>('mask_apply', { doc, id }),
  maskSet: (doc: number, id: number, props: { enabled?: boolean; density?: number; feather?: number; invert?: boolean }, transient = false) =>
    invoke<DocInfo>('mask_set', { doc, id, props, transient }),
  maskToSelection: (doc: number, id: number) => invoke<DocInfo>('mask_to_selection', { doc, id }),

  liveBegin: (doc: number, layer: number) => invoke<number | null>('live_begin', { doc, layer }),
  liveEnd: (session: number) => invoke<void>('live_end', { session }),

  undo: (doc: number) => invoke<DocInfo>('undo', { doc }),
  redo: (doc: number) => invoke<DocInfo>('redo', { doc }),
  historyJump: (doc: number, index: number) => invoke<DocInfo>('history_jump', { doc, index }),

  recoveryList: () => invoke<RecoveredFile[]>('recovery_list'),
  recoveryOpen: (file: string) => invoke<DocInfo>('recovery_open', { file }),
  recoveryDiscard: (file: string) => invoke<void>('recovery_discard', { file }),

  debugTestDocument: (width: number, height: number, layers: number) => invoke<DocInfo>('debug_test_document', { width, height, layers }),
  debugDocHash: (doc: number) => invoke<string>('debug_doc_hash', { doc }),
  /** [backdrop, source, CPU result], each 256×256 RGBA8, for the GPU-vs-CPU blend test. */
  debugBlendReference: async (mode: string, opacity: number) => new Uint8Array(await invoke<ArrayBuffer>('debug_blend_reference', { mode, opacity })),
};

/** Walks a layer tree (top to bottom), calling `f` with each layer and its parent group id. */
export function walkLayers(layers: LayerInfo[], f: (l: LayerInfo, parent: number | null, depth: number) => void, parent: number | null = null, depth = 0) {
  for (const l of layers) {
    f(l, parent, depth);
    if (l.children) walkLayers(l.children, f, l.id, depth + 1);
  }
}

export function findLayer(layers: LayerInfo[], id: number): LayerInfo | null {
  let found: LayerInfo | null = null;
  walkLayers(layers, (l) => {
    if (l.id === id) found = l;
  });
  return found;
}
