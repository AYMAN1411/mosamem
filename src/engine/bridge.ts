// Typed calls into the native engine.
import { invoke } from '@tauri-apps/api/core';
import type { BlendMode } from './blend';

export interface LayerInfo {
  id: number;
  name: string;
  visible: boolean;
  opacity: number;
  blend: BlendMode;
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
}

export type Background = { kind: 'white' } | { kind: 'black' } | { kind: 'transparent' } | { kind: 'color'; rgba: [number, number, number, number] };

export type ExportFormat = 'png' | 'jpeg' | 'webp' | 'tiff' | 'heic';

const isWindows = navigator.userAgent.includes('Windows');

/** URL of one flattened tile. Windows WebView2 serves custom schemes as http://<scheme>.localhost. */
export function tileUrl(doc: number, version: number, lod: number, tx: number, ty: number): string {
  const base = isWindows ? 'http://lawha.localhost' : 'lawha://localhost';
  return `${base}/composite/${doc}/${version}/${lod}/${tx}/${ty}`;
}

export const engine = {
  newDocument: (spec: { name: string; width: number; height: number; ppi: number; background: Background; backgroundLayerName: string }) =>
    invoke<DocInfo>('new_document', { spec }),
  openImage: (path: string, layerName: string) => invoke<DocInfo>('open_image', { path, layerName }),
  exportImage: (spec: { doc: number; path: string; format?: ExportFormat; quality: number; lossless: boolean }) => invoke<void>('export_image', { spec }),
  closeDocument: (doc: number) => invoke<void>('close_document', { doc }),
  documentInfo: (doc: number) => invoke<DocInfo>('document_info', { doc }),
  debugTestDocument: (width: number, height: number, layers: number) => invoke<DocInfo>('debug_test_document', { width, height, layers }),
};
