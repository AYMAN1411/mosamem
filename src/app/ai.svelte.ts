// Local AI in the interface: Select Subject, Remove Background, Object Selection and Smart Fill.
// A feature whose model isn't on the disk yet asks first (AiModelDialog), downloads it with
// progress, then runs. Everything runs on the device; see src-tauri/src/ai.
import { app } from './state.svelte';
import { t, type Key } from './i18n/index.svelte';
import { engine, findLayer, type AiPack, type AiPackStatus, type AiClick, type Combine, type DocInfo } from '../engine/bridge';
import { message } from './actions';

class AiState {
  packs = $state<AiPackStatus[]>([]);
  /** The pack the download dialog is asking about. */
  asking = $state<AiPack | null>(null);
  /** What's running, shown over the canvas. */
  working = $state<string | null>(null);
  private answer: ((ok: boolean) => void) | null = null;

  async refresh() {
    try {
      this.packs = await engine.aiStatus();
    } catch (e) {
      console.warn('ai status', e);
    }
    return this.packs;
  }

  installed(pack: AiPack) {
    return this.packs.find((p) => p.id === pack)?.installed ?? false;
  }

  /** Resolves true once the pack is on the disk (asking the user to download it if needed). */
  async ensure(pack: AiPack): Promise<boolean> {
    await this.refresh();
    if (this.installed(pack)) return true;
    this.asking = pack;
    app.dialog = 'aiModel';
    return new Promise((resolve) => (this.answer = resolve));
  }

  /** Called by the dialog when it closes. */
  done(ok: boolean) {
    this.asking = null;
    if (app.dialog === 'aiModel') app.dialog = null;
    this.answer?.(ok);
    this.answer = null;
    void this.refresh();
  }
}

export const ai = new AiState();

/** Engine errors with a meaning for the user. */
const KNOWN: Record<string, Key> = {
  'no-selection': 'ai.err.noSelection',
  'not-pixel-layer': 'ai.err.notPixel',
  'empty-layer': 'ai.err.emptyLayer',
};

async function run(pack: AiPack | null, label: Key, f: (doc: number) => Promise<DocInfo>) {
  const d = app.active;
  if (!d || ai.working) return;
  if (pack && !(await ai.ensure(pack))) return;
  ai.working = t(label);
  try {
    app.update(await f(d.info.id));
  } catch (e) {
    const m = message(e);
    if (m.startsWith('model-missing:')) {
      // removed while we were asking: ask again
      ai.working = null;
      return run(pack, label, f);
    }
    app.notify(KNOWN[m] ? t(KNOWN[m]) : t('err.generic', { msg: m }), 'error');
  } finally {
    ai.working = null;
  }
}

export const aiActions = {
  selectSubject(mode: Combine = app.sel.mode) {
    return run('subject', 'ai.working.subject', (doc) => engine.aiSelectSubject(doc, mode));
  },
  removeBackground() {
    const d = app.active;
    if (!d) return;
    const l = findLayer(d.info.layers, d.info.active);
    if (l?.kind !== 'pixel') {
      app.notify(t('ai.err.notPixel'), 'error');
      return;
    }
    return run('subject', 'ai.working.background', (doc) => engine.aiRemoveBackground(doc, l.id));
  },
  objectSelect(clicks: AiClick[], bbox: [number, number, number, number] | null, mode: Combine) {
    return run('select', 'ai.working.object', (doc) => engine.aiObjectSelect(doc, { clicks, bbox, mode, allLayers: app.sel.allLayers }));
  },
  smartFill(method: 'ai' | 'classic', newLayer: boolean) {
    return run(method === 'ai' ? 'fill' : null, 'ai.working.fill', (doc) => engine.aiSmartFill(doc, { method, newLayer: newLayer ? t('ai.fillLayer') : null }));
  },
  openSmartFill() {
    // Shift+Backspace while typing deletes text; it's only Smart Fill on the canvas
    const el = document.activeElement;
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || (el instanceof HTMLElement && el.isContentEditable)) {
      document.execCommand('delete');
      return;
    }
    if (!app.active?.info.hasSelection) {
      app.notify(t('ai.err.noSelection'), 'error');
      return;
    }
    void ai.refresh();
    app.dialog = 'smartFill';
  },
  openModels() {
    void ai.refresh();
    app.dialog = 'aiModels';
  },
};

/** Computes the image embedding ahead of the first click, while the Object Selection tool is up
 * and its model is installed (quietly; a failure just means the click does it). */
let prepared = '';
export function prepareObjectSelect() {
  const d = app.active;
  if (!d || app.tool !== 'objectSelect' || !ai.installed('select')) return;
  const key = `${d.info.id}:${d.info.version}:${app.sel.allLayers}`;
  if (key === prepared) return;
  prepared = key;
  engine.aiObjectPrepare(d.info.id, app.sel.allLayers).catch(() => (prepared = ''));
}

export function formatBytes(n: number): string {
  const mb = n / 1e6;
  return mb >= 1000 ? `${(mb / 1000).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}
