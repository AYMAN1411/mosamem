// User-level actions, shared by menus, buttons and shortcuts.
import { open, save } from '@tauri-apps/plugin-dialog';
import { engine, type ExportFormat } from '../engine/bridge';
import { app } from './state.svelte';
import { t } from './i18n/index.svelte';

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp', 'tif', 'tiff', 'heic', 'heif'];

function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

export async function openPaths(paths: string[]) {
  for (const path of paths) {
    try {
      app.addDocument(await engine.openImage(path, t('layer.background')));
    } catch (e) {
      app.notify(t('err.open', { msg: message(e) }), 'error');
    }
  }
}

export async function openFile() {
  const picked = await open({ multiple: true, filters: [{ name: t('files.images'), extensions: IMAGE_EXTENSIONS }] });
  if (!picked) return;
  await openPaths(Array.isArray(picked) ? picked : [picked]);
}

export function newDocument() {
  app.dialog = 'new';
}

export async function exportAs(format: ExportFormat, quality: number, lossless: boolean) {
  const d = app.active;
  if (!d) return;
  const ext = format === 'jpeg' ? 'jpg' : format;
  const base = d.info.name.replace(/\.[^.]+$/, '') || 'image';
  const path = await save({ defaultPath: `${base}.${ext}`, filters: [{ name: format.toUpperCase(), extensions: [ext] }] });
  if (!path) return false;
  try {
    await engine.exportImage({ doc: d.info.id, path, format, quality, lossless });
    app.notify(t('export.done', { name: path.split(/[\\/]/).pop()! }));
    return true;
  } catch (e) {
    app.notify(t('err.export', { msg: message(e) }), 'error');
    return false;
  }
}

export async function closeDocument(id = app.activeId) {
  if (id == null) return;
  app.removeDocument(id);
  await engine.closeDocument(id);
}

export const view = {
  zoomIn: () => app.zoomStep(1),
  zoomOut: () => app.zoomStep(-1),
  fit: () => app.fit(),
  actual: () => app.actualPixels(),
  resetRotation: () => app.rotateTo(0),
  toggleRulers: () => (app.showRulers = !app.showRulers),
  toggleGrid: () => (app.showGrid = !app.showGrid),
  toggleGuides: () => (app.showGuides = !app.showGuides),
  togglePixelGrid: () => (app.showPixelGrid = !app.showPixelGrid),
  clearGuides: () => {
    if (app.active) app.active.guides = [];
  },
};
