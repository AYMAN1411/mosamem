// App-wide UI state. Pixels and the document model live in the native engine; this holds what the
// UI shows and how the user is viewing it.
import type { FilterKind } from '../engine/filter';
import type { DocInfo } from '../engine/bridge';

export type ToolId =
  | 'move'
  | 'marqueeRect'
  | 'marqueeEllipse'
  | 'lasso'
  | 'lassoPolygon'
  | 'magicWand'
  | 'quickSelect'
  | 'crop'
  | 'eyedropper'
  | 'heal'
  | 'spotHeal'
  | 'brush'
  | 'clone'
  | 'eraser'
  | 'gradient'
  | 'bucket'
  | 'text'
  | 'hand'
  | 'zoom'
  | 'rotateView';

export interface Guide {
  id: number;
  axis: 'h' | 'v'; // h = horizontal line at y = pos; v = vertical line at x = pos
  pos: number; // document px
}

export interface ViewState {
  /** CSS px per document px. */
  zoom: number;
  cx: number;
  cy: number;
  /** Degrees, clockwise. */
  rotation: number;
}

export interface DocState {
  info: DocInfo;
  view: ViewState;
  guides: Guide[];
  /** True until the user pans or zooms; while true, resizing the window refits the document. */
  fitted: boolean;
  /** Layers selected in the Layers panel (the active layer is always one of them). */
  selection: number[];
}

export const ZOOM_STEPS = [0.01, 0.02, 0.03, 0.05, 0.0667, 0.0833, 0.125, 0.1667, 0.25, 0.3333, 0.5, 0.6667, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 32, 64];
export const MIN_ZOOM = 0.01;
export const MAX_ZOOM = 64;

let guideId = 1;
export const nextGuideId = () => guideId++;

export interface BrushSettings {
  diameter: number;
  /** 0–1 */
  hardness: number;
  opacity: number;
  flow: number;
  /** fraction of the diameter */
  spacing: number;
  /** 0–1: leash length for stroke smoothing */
  smoothing: number;
  pressureSize: boolean;
  pressureOpacity: boolean;
  minSize: number;
}

const brushDefaults = (o: Partial<BrushSettings> = {}): BrushSettings => ({
  diameter: 30, hardness: 0, opacity: 1, flow: 1, spacing: 0.1, smoothing: 0.1, pressureSize: true, pressureOpacity: false, minSize: 0.1, ...o,
});

export interface PaintOptions {
  brush: BrushSettings;
  eraser: BrushSettings;
  clone: BrushSettings & { aligned: boolean; sampleAll: boolean };
  heal: BrushSettings & { sampleAll: boolean };
  gradient: { kind: 'linear' | 'radial' | 'angle' | 'reflected' | 'diamond'; reverse: boolean; dither: boolean; opacity: number; toTransparent: boolean };
  bucket: { tolerance: number; contiguous: boolean; allLayers: boolean; antialias: boolean; opacity: number };
  eyedropper: { size: number; allLayers: boolean };
}

export interface SelectionOptions {
  mode: 'replace' | 'add' | 'subtract' | 'intersect';
  feather: number;
  antialias: boolean;
  style: 'normal' | 'ratio' | 'size';
  ratioW: number;
  ratioH: number;
  sizeW: number;
  sizeH: number;
  tolerance: number;
  contiguous: boolean;
  allLayers: boolean;
  /** Quick selection brush diameter, screen-independent document px. */
  brush: number;
}

class AppState {
  docs = $state<DocState[]>([]);
  activeId = $state<number | null>(null);
  tool = $state<ToolId>('move');
  fg = $state('#000000');
  bg = $state('#ffffff');
  showRulers = $state(true);
  showGrid = $state(false);
  showGuides = $state(true);
  showPixelGrid = $state(true);
  gridSpacing = $state(100);
  paint = $state<PaintOptions>({
    brush: brushDefaults(),
    eraser: brushDefaults({ diameter: 60, hardness: 0.5 }),
    clone: { ...brushDefaults({ diameter: 60, hardness: 0.3 }), aligned: true, sampleAll: false },
    heal: { ...brushDefaults({ diameter: 40, hardness: 0.6 }), sampleAll: false },
    gradient: { kind: 'linear', reverse: false, dither: true, opacity: 1, toTransparent: false },
    bucket: { tolerance: 32, contiguous: true, allLayers: false, antialias: true, opacity: 1 },
    eyedropper: { size: 1, allLayers: true },
  });
  sel = $state<SelectionOptions>({ mode: 'replace', feather: 0, antialias: true, style: 'normal', ratioW: 1, ratioH: 1, sizeW: 1080, sizeH: 1080, tolerance: 32, contiguous: true, allLayers: false, brush: 30 });
  gridSubdivisions = $state(4);
  dialog = $state<null | 'new' | 'export' | 'recovery' | 'unsaved' | 'modify' | 'refine' | 'filter'>(null);
  /** The filter the filter panel is showing. */
  filterKind = $state<FilterKind>('gaussianBlur');
  modifyOp = $state<'feather' | 'expand' | 'contract' | 'border' | 'smooth'>('feather');
  /** True when the layer mask (not the pixels) of the active layer is the editing target. */
  maskTarget = $state(false);
  toast = $state<{ text: string; kind: 'info' | 'error' } | null>(null);
  /** Size of the canvas area in CSS px, kept current by the canvas view. */
  viewport = $state({ w: 800, h: 600 });
  untitledCount = 0;

  get active(): DocState | null {
    return this.docs.find((d) => d.info.id === this.activeId) ?? null;
  }

  addDocument(info: DocInfo) {
    const d: DocState = { info, view: { zoom: 1, cx: info.width / 2, cy: info.height / 2, rotation: 0 }, guides: [], fitted: true, selection: [info.active] };
    this.docs.push(d);
    this.activeId = info.id;
    this.fit();
  }

  /** Applies a new document state returned by the engine. */
  update(info: DocInfo) {
    const d = this.docs.find((x) => x.info.id === info.id);
    if (!d) return;
    if (info.width !== d.info.width || info.height !== d.info.height) d.fitted = true;
    d.info = info;
    // keep the selection valid: drop removed layers and always include the active one
    const ids = new Set<number>();
    const walk = (ls: DocInfo['layers']) => ls.forEach((l) => (ids.add(l.id), l.children && walk(l.children)));
    walk(info.layers);
    d.selection = d.selection.filter((x) => ids.has(x));
    if (!d.selection.includes(info.active)) d.selection = [info.active];
    if (d.fitted) this.fit();
  }

  removeDocument(id: number) {
    const i = this.docs.findIndex((d) => d.info.id === id);
    if (i < 0) return;
    this.docs.splice(i, 1);
    if (this.activeId === id) this.activeId = this.docs[Math.min(i, this.docs.length - 1)]?.info.id ?? null;
  }

  fitZoom(d: DocState): number {
    const pad = 48;
    const z = Math.min((this.viewport.w - pad) / d.info.width, (this.viewport.h - pad) / d.info.height);
    return Math.max(MIN_ZOOM, Math.min(z, 64));
  }

  fit() {
    const d = this.active;
    if (!d) return;
    d.view = { zoom: this.fitZoom(d), cx: d.info.width / 2, cy: d.info.height / 2, rotation: d.view.rotation };
    d.fitted = true;
  }

  actualPixels() {
    const d = this.active;
    if (!d) return;
    d.view = { ...d.view, zoom: 1 };
    d.fitted = false;
  }

  /** Zoom keeping the document point under the given viewport point (CSS px) fixed. */
  zoomTo(zoom: number, at?: { x: number; y: number }) {
    const d = this.active;
    if (!d) return;
    const z = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, zoom));
    const v = d.view;
    if (at) {
      const p = viewportToDoc(v, this.viewport, at.x, at.y);
      // after zoom, p must stay at `at`: solve for the new center
      const r = (v.rotation * Math.PI) / 180;
      const dx = (at.x - this.viewport.w / 2) / z;
      const dy = (at.y - this.viewport.h / 2) / z;
      const cos = Math.cos(-r);
      const sin = Math.sin(-r);
      d.view = { ...v, zoom: z, cx: p.x - (cos * dx - sin * dy), cy: p.y - (sin * dx + cos * dy) };
    } else {
      d.view = { ...v, zoom: z };
    }
    d.fitted = false;
  }

  zoomStep(dir: 1 | -1, at?: { x: number; y: number }) {
    const d = this.active;
    if (!d) return;
    const z = d.view.zoom;
    const next = dir > 0 ? ZOOM_STEPS.find((s) => s > z * 1.001) ?? MAX_ZOOM : [...ZOOM_STEPS].reverse().find((s) => s < z / 1.001) ?? MIN_ZOOM;
    this.zoomTo(next, at);
  }

  panBy(dxCss: number, dyCss: number) {
    const d = this.active;
    if (!d) return;
    const r = (d.view.rotation * Math.PI) / 180;
    const cos = Math.cos(-r);
    const sin = Math.sin(-r);
    const dx = dxCss / d.view.zoom;
    const dy = dyCss / d.view.zoom;
    d.view = { ...d.view, cx: d.view.cx - (cos * dx - sin * dy), cy: d.view.cy - (sin * dx + cos * dy) };
    d.fitted = false;
  }

  rotateTo(deg: number) {
    const d = this.active;
    if (!d) return;
    let r = deg % 360;
    if (r > 180) r -= 360;
    if (r <= -180) r += 360;
    d.view = { ...d.view, rotation: r };
  }

  notify(text: string, kind: 'info' | 'error' = 'info') {
    this.toast = { text, kind };
    const current = this.toast;
    setTimeout(() => {
      if (this.toast === current) this.toast = null;
    }, kind === 'error' ? 6000 : 3000);
  }
}

/** Viewport CSS px → document px for a view. */
export function viewportToDoc(v: ViewState, vp: { w: number; h: number }, x: number, y: number) {
  const r = (v.rotation * Math.PI) / 180;
  const dx = (x - vp.w / 2) / v.zoom;
  const dy = (y - vp.h / 2) / v.zoom;
  const cos = Math.cos(-r);
  const sin = Math.sin(-r);
  return { x: v.cx + cos * dx - sin * dy, y: v.cy + sin * dx + cos * dy };
}

/** Document px → viewport CSS px. */
export function docToViewport(v: ViewState, vp: { w: number; h: number }, x: number, y: number) {
  const r = (v.rotation * Math.PI) / 180;
  const dx = (x - v.cx) * v.zoom;
  const dy = (y - v.cy) * v.zoom;
  const cos = Math.cos(r);
  const sin = Math.sin(r);
  return { x: vp.w / 2 + cos * dx - sin * dy, y: vp.h / 2 + sin * dx + cos * dy };
}

export const app = new AppState();
