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
}

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
