// User-level actions, shared by menus, buttons and shortcuts.
import { open, save } from '@tauri-apps/plugin-dialog';
import { engine, findLayer, type DocInfo, type ExportFormat, type LayerProps, type Modify, type NewMask } from '../engine/bridge';
import { app, type DocState } from './state.svelte';
import { t } from './i18n/index.svelte';

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp', 'tif', 'tiff', 'heic', 'heif'];

export function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

/** Runs an engine call for the active document and applies the returned state. */
async function run(f: (d: DocState) => Promise<DocInfo>) {
  const d = app.active;
  if (!d) return;
  try {
    app.update(await f(d));
  } catch (e) {
    app.notify(t('err.generic', { msg: message(e) }), 'error');
  }
}

// ---- files ----

export async function openPaths(paths: string[]) {
  for (const path of paths) {
    const already = app.docs.find((d) => d.info.path === path);
    if (already) {
      app.activeId = already.info.id;
      continue;
    }
    try {
      app.addDocument(await engine.openFile(path, t('layer.background')));
    } catch (e) {
      app.notify(t('err.open', { msg: message(e) }), 'error');
    }
  }
}

export async function openFile() {
  const picked = await open({
    multiple: true,
    filters: [{ name: `${t('files.images')} / ${t('save.lawhaFiles')}`, extensions: ['lawha', ...IMAGE_EXTENSIONS] }],
  });
  if (!picked) return;
  await openPaths(Array.isArray(picked) ? picked : [picked]);
}

export function newDocument() {
  app.dialog = 'new';
}

/** Saves as .lawha. Returns false if the user cancelled or saving failed. */
export async function saveDocument(d: DocState | null = app.active, saveAs = false): Promise<boolean> {
  if (!d) return false;
  let path = d.info.path && d.info.path.endsWith('.lawha') && !saveAs ? d.info.path : null;
  if (!path) {
    const base = d.info.name.replace(/\.[^.]+$/, '') || 'image';
    path = await save({ defaultPath: `${base}.lawha`, filters: [{ name: t('save.lawhaFiles'), extensions: ['lawha'] }] });
    if (!path) return false;
  }
  try {
    app.update(await engine.saveDocument(d.info.id, path));
    app.notify(t('save.saved', { name: path.split(/[\\/]/).pop()! }));
    return true;
  } catch (e) {
    app.notify(t('err.save', { msg: message(e) }), 'error');
    return false;
  }
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

/** Documents waiting for a save/don't-save answer, and what happens after. */
export const pendingClose: { docs: number[]; then: (() => void) | null } = { docs: [], then: null };

/** Closes a document, asking about unsaved changes first. */
export async function closeDocument(id = app.activeId) {
  if (id == null) return;
  const d = app.docs.find((x) => x.info.id === id);
  if (d?.info.dirty) {
    pendingClose.docs = [id];
    pendingClose.then = null;
    app.dialog = 'unsaved';
    return;
  }
  await forceClose(id);
}

export async function forceClose(id: number) {
  app.removeDocument(id);
  await engine.closeDocument(id);
}

// ---- layers ----

const activeId = () => app.active?.info.active ?? 0;
const selection = () => app.active?.selection ?? [];

let groupCount = 0;

export const layer = {
  new: () => run((d) => engine.layerNew(d.info.id, t('layer.name', { n: countLayers(d.info) }))),
  newGroup: () => run((d) => engine.layerGroup(d.info.id, t('layer.groupName', { n: ++groupCount }), [])),
  group: () => run((d) => engine.layerGroup(d.info.id, t('layer.groupName', { n: ++groupCount }), selection())),
  ungroup: () =>
    run((d) => {
      const l = findLayer(d.info.layers, activeId());
      return l?.kind === 'group' ? engine.layerUngroup(d.info.id, l.id) : Promise.resolve(d.info);
    }),
  duplicate: () => run((d) => engine.layerDuplicate(d.info.id, selection(), t('layer.copySuffix'))),
  delete: () => run((d) => engine.layerDelete(d.info.id, selection())),
  mergeDown: () => run((d) => engine.layerMergeDown(d.info.id, activeId())),
  mergeVisible: () => run((d) => engine.layerMergeVisible(d.info.id, t('layer.merged'))),
  flatten: () => run((d) => engine.layerFlatten(d.info.id, t('layer.background'))),
  toggleClip: () =>
    run((d) => {
      const l = findLayer(d.info.layers, activeId());
      return l ? engine.layerSet(d.info.id, l.id, { clipped: !l.clipped }) : Promise.resolve(d.info);
    }),
  set: (id: number, props: LayerProps, transient = false) => run((d) => engine.layerSet(d.info.id, id, props, transient)),
  solo: (id: number) => run((d) => engine.layerSolo(d.info.id, id)),
  move: (ids: number[], parent: number | null, index: number) => run((d) => engine.layerMove(d.info.id, ids, parent, index)),
  activate: (id: number) => run((d) => engine.setActive(d.info.id, id)),
};

function countLayers(info: DocInfo): number {
  let n = 0;
  const walk = (ls: DocInfo['layers']) => ls.forEach((l) => (l.kind === 'pixel' && n++, l.children && walk(l.children)));
  walk(info.layers);
  return n;
}

// ---- history ----

export const history = {
  undo: () => run((d) => engine.undo(d.info.id)),
  redo: () => run((d) => engine.redo(d.info.id)),
  jump: (i: number) => run((d) => engine.historyJump(d.info.id, i)),
};

// ---- view ----

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

// ---- selection ----

export const select = {
  all: () => run((d) => engine.selectSimple(d.info.id, 'all')),
  deselect: () => run((d) => engine.selectSimple(d.info.id, 'deselect')),
  reselect: () => run((d) => engine.selectSimple(d.info.id, 'reselect')),
  invert: () => run((d) => engine.selectSimple(d.info.id, 'invert')),
  modify: (op: Modify) => run((d) => engine.selectModify(d.info.id, op)),
  /** Opens the small amount dialog for a Modify command. */
  ask: (op: Modify['op']) => {
    app.modifyOp = op;
    app.dialog = 'modify';
  },
  refine: () => {
    if (app.active?.info.hasSelection) app.dialog = 'refine';
  },
};

// ---- layer masks ----

export const mask = {
  add: (kind: NewMask) => run((d) => engine.maskAdd(d.info.id, d.info.active, kind)),
  /** The panel's mask button: from the selection if there is one; Alt = the hiding variant. */
  addAuto: (alt: boolean) => {
    const sel = app.active?.info.hasSelection;
    return mask.add(sel ? (alt ? 'hideSelection' : 'revealSelection') : alt ? 'hideAll' : 'revealAll');
  },
  delete: () => run((d) => engine.maskDelete(d.info.id, d.info.active)),
  apply: () => run((d) => engine.maskApply(d.info.id, d.info.active)),
  toggle: (id?: number) =>
    run((d) => {
      const l = findLayer(d.info.layers, id ?? d.info.active);
      return l?.mask ? engine.maskSet(d.info.id, l.id, { enabled: !l.mask.enabled }) : Promise.resolve(d.info);
    }),
  invert: () => run((d) => engine.maskSet(d.info.id, d.info.active, { invert: true })),
  set: (props: { density?: number; feather?: number }, transient = false) => run((d) => engine.maskSet(d.info.id, d.info.active, props, transient)),
  toSelection: (id: number) => run((d) => engine.maskToSelection(d.info.id, id)),
};
