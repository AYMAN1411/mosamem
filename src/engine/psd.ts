// PSD ⇄ Mosamem, behind one interface (docs/plan.md: `PsdCodec`). The file format work is ag-psd's
// (MIT); this module maps its layer objects to the engine's manifest (src-tauri/src/doc/psd.rs) and
// back. It runs in a worker, so it must not touch the DOM or Tauri.
import { initializeCanvas, readPsd, writePsdUint8Array } from 'ag-psd';
import type { BlendMode as PsdBlend, Color, Layer, LayerEffectsInfo, LayerMaskData, LayerVectorMask, Psd, AdjustmentLayer, LayerTextData } from 'ag-psd';
import type { Adjustment } from './adjust';
import type { BlendMode } from './blend';
import type { LayerStyle } from './style';
import type { TextData, Family } from './text';
import { shapePath, type Path, type ShapeData, type Stroke } from './vector';

// ag-psd asks for canvases; with `useImageData` it only needs plain pixel buffers
initializeCanvas(
  () => {
    throw new Error('no canvas in the PSD worker');
  },
  (width: number, height: number) => ({ width, height, data: new Uint8ClampedArray(width * height * 4), colorSpace: 'srgb' }) as unknown as ImageData,
);

export interface Pixels { offset: number; x: number; y: number; w: number; h: number }
export interface PsdMask { pixels: Pixels | null; defaultColor: number; enabled: boolean; density: number; feather: number }
export interface PsdLayer {
  name: string;
  kind: 'pixel' | 'group' | 'adjustment' | 'text' | 'shape';
  visible: boolean;
  opacity: number;
  fill: number;
  blend: BlendMode;
  passThrough: boolean;
  clipped: boolean;
  locks: { transparent: boolean; pixels: boolean; position: boolean };
  expanded: boolean;
  pixels: Pixels | null;
  mask: PsdMask | null;
  children: PsdLayer[];
  adjustment: Adjustment | null;
  text: TextData | null;
  shape: ShapeData | null;
  style: LayerStyle | null;
}
export interface PsdDoc { width: number; height: number; ppi: number; layers: PsdLayer[]; composite: Pixels | null }

/** What the reader saw but couldn't carry over (for the user and the round-trip report). */
/** A warning's i18n key (under `psdw.`) and its values. */
export interface PsdWarning { key: string; params?: Record<string, string | number> }
type Warn = (key: string, params?: Record<string, string | number>) => void;
export interface ReadResult { message: Uint8Array; composite: Pixels | null; warnings: PsdWarning[]; features: string[] }

// ---- wire format: [u32 manifest length][manifest JSON][pixels] ----

class Packer {
  chunks: Uint8Array[] = [];
  size = 0;
  push(data: Uint8Array): number {
    const at = this.size;
    this.chunks.push(data);
    this.size += data.byteLength;
    return at;
  }
}

function pack(doc: PsdDoc, px: Packer): Uint8Array {
  const json = new TextEncoder().encode(JSON.stringify(doc));
  const out = new Uint8Array(4 + json.length + px.size);
  new DataView(out.buffer).setUint32(0, json.length, true);
  out.set(json, 4);
  let at = 4 + json.length;
  for (const c of px.chunks) {
    out.set(c, at);
    at += c.byteLength;
  }
  return out;
}

export function unpack(message: Uint8Array): { doc: PsdDoc; pixels: Uint8Array } {
  const n = new DataView(message.buffer, message.byteOffset).getUint32(0, true);
  const doc = JSON.parse(new TextDecoder().decode(message.subarray(4, 4 + n))) as PsdDoc;
  return { doc, pixels: message.subarray(4 + n) };
}

// ---- helpers ----

const srgb = (v: number) => (v <= 0.0031308 ? v * 12.92 : 1.055 * Math.pow(v, 1 / 2.4) - 0.055);
const bytes8 = (data: ArrayLike<number> & { buffer: ArrayBufferLike; byteOffset: number; byteLength: number }, bpc: number, channels = 4): Uint8Array => {
  if (bpc <= 8) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  const src = data as unknown as ArrayLike<number>;
  const out = new Uint8Array(src.length);
  if (bpc === 16) {
    for (let i = 0; i < src.length; i++) out[i] = Math.round(src[i] / 257);
    return out;
  }
  // 32-bit files hold linear light: encode color as sRGB, keep alpha linear
  for (let i = 0; i < src.length; i++) {
    const v = Math.max(0, Math.min(1, src[i]));
    out[i] = Math.round((channels === 4 && i % 4 === 3 ? v : srgb(v)) * 255);
  }
  return out;
};

const BLENDS: Record<string, BlendMode> = {
  normal: 'normal', dissolve: 'dissolve', darken: 'darken', multiply: 'multiply', 'color burn': 'colorBurn', 'linear burn': 'linearBurn', 'darker color': 'darkerColor',
  lighten: 'lighten', screen: 'screen', 'color dodge': 'colorDodge', 'linear dodge': 'linearDodge', 'lighter color': 'lighterColor', overlay: 'overlay', 'soft light': 'softLight',
  'hard light': 'hardLight', 'vivid light': 'vividLight', 'linear light': 'linearLight', 'pin light': 'pinLight', 'hard mix': 'hardMix', difference: 'difference', exclusion: 'exclusion',
  subtract: 'subtract', divide: 'divide', hue: 'hue', saturation: 'saturation', color: 'color', luminosity: 'luminosity',
};
const toBlend = (b: string | undefined): BlendMode => BLENDS[b ?? 'normal'] ?? 'normal';
const fromBlend = (b: BlendMode): PsdBlend => (Object.keys(BLENDS).find((k) => BLENDS[k] === b) ?? 'normal') as PsdBlend;

function rgb(c: Color | undefined, fallback: [number, number, number] = [0, 0, 0]): [number, number, number] {
  if (!c) return fallback;
  const o = c as Record<string, number>;
  if ('r' in o) return [Math.round(o.r), Math.round(o.g), Math.round(o.b)];
  if ('fr' in o) return [Math.round(o.fr * 255), Math.round(o.fg * 255), Math.round(o.fb * 255)];
  if ('k' in o && !('c' in o)) {
    const v = Math.round(255 - (o.k / 100) * 255);
    return [v, v, v];
  }
  return fallback;
}
const psdColor = (c: number[]) => ({ r: c[0], g: c[1], b: c[2] });
const px = (v: { value: number } | undefined, d: number) => v?.value ?? d;

// ---- adjustments ----

function readAdjustment(a: AdjustmentLayer, warn: Warn): Adjustment | null {
  const lv = (c?: { shadowInput: number; highlightInput: number; shadowOutput: number; highlightOutput: number; midtoneInput: number }) =>
    c ? { inBlack: c.shadowInput, inWhite: c.highlightInput, gamma: c.midtoneInput, outBlack: c.shadowOutput, outWhite: c.highlightOutput } : { inBlack: 0, inWhite: 255, gamma: 1, outBlack: 0, outWhite: 255 };
  const cv = (c?: { input: number; output: number }[]) => (c && c.length >= 2 ? c.map((p) => [p.input, p.output] as [number, number]) : ([[0, 0], [255, 255]] as [number, number][]));
  const hsl = (c?: { hue: number; saturation: number; lightness: number }) => ({ hue: c?.hue ?? 0, saturation: c?.saturation ?? 0, lightness: c?.lightness ?? 0 });
  const cb = (c?: { cyanRed: number; magentaGreen: number; yellowBlue: number }) => [c?.cyanRed ?? 0, c?.magentaGreen ?? 0, c?.yellowBlue ?? 0];
  switch (a.type) {
    case 'levels':
      return { kind: 'levels', channels: [lv(a.rgb), lv(a.red), lv(a.green), lv(a.blue)] };
    case 'curves':
      return { kind: 'curves', channels: [cv(a.rgb), cv(a.red), cv(a.green), cv(a.blue)] };
    case 'hue/saturation':
      return { kind: 'hueSaturation', master: hsl(a.master), ranges: [hsl(a.reds), hsl(a.yellows), hsl(a.greens), hsl(a.cyans), hsl(a.blues), hsl(a.magentas)], colorize: false, colorizeHsl: { hue: 0, saturation: 25, lightness: 0 } };
    case 'brightness/contrast':
      return { kind: 'brightnessContrast', brightness: a.brightness ?? 0, contrast: a.contrast ?? 0, legacy: !!a.useLegacy };
    case 'exposure':
      return { kind: 'exposure', exposure: a.exposure ?? 0, offset: a.offset ?? 0, gamma: a.gamma ?? 1 };
    case 'vibrance':
      return { kind: 'vibrance', vibrance: a.vibrance ?? 0, saturation: a.saturation ?? 0 };
    case 'color balance':
      return { kind: 'colorBalance', shadows: cb(a.shadows), midtones: cb(a.midtones), highlights: cb(a.highlights), preserveLuminosity: a.preserveLuminosity ?? true };
    case 'black & white':
      return { kind: 'blackWhite', weights: [a.reds ?? 40, a.yellows ?? 60, a.greens ?? 40, a.cyans ?? 60, a.blues ?? 20, a.magentas ?? 80], tint: !!a.useTint, tintColor: rgb(a.tintColor, [225, 211, 179]) };
    case 'gradient map':
      if (a.gradientType === 'noise') warn('noiseGradientMap');
      return { kind: 'gradientMap', stops: (a.colorStops ?? []).map((s) => ({ pos: s.location, color: rgb(s.color) })), reverse: !!a.reverse, dither: !!a.dither };
    case 'invert':
      return { kind: 'invert' };
    case 'threshold':
      return { kind: 'threshold', level: a.level ?? 128 };
    default:
      warn('adjustment', { type: a.type });
      return null;
  }
}

function writeAdjustment(a: Adjustment): AdjustmentLayer {
  const lv = (c: { inBlack: number; inWhite: number; gamma: number; outBlack: number; outWhite: number }) => ({ shadowInput: c.inBlack, highlightInput: c.inWhite, midtoneInput: c.gamma, shadowOutput: c.outBlack, highlightOutput: c.outWhite });
  const cv = (c: [number, number][]) => c.map(([input, output]) => ({ input, output }));
  const hsl = (c: { hue: number; saturation: number; lightness: number }) => ({ a: 0, b: 0, c: 0, d: 0, ...c });
  const cb = (c: number[]) => ({ cyanRed: c[0], magentaGreen: c[1], yellowBlue: c[2] });
  switch (a.kind) {
    case 'levels':
      return { type: 'levels', rgb: lv(a.channels[0]), red: lv(a.channels[1]), green: lv(a.channels[2]), blue: lv(a.channels[3]) };
    case 'curves':
      return { type: 'curves', rgb: cv(a.channels[0]), red: cv(a.channels[1]), green: cv(a.channels[2]), blue: cv(a.channels[3]) };
    case 'hueSaturation':
      return { type: 'hue/saturation', master: hsl(a.master), reds: hsl(a.ranges[0]), yellows: hsl(a.ranges[1]), greens: hsl(a.ranges[2]), cyans: hsl(a.ranges[3]), blues: hsl(a.ranges[4]), magentas: hsl(a.ranges[5]) };
    case 'brightnessContrast':
      return { type: 'brightness/contrast', brightness: a.brightness, contrast: a.contrast, useLegacy: a.legacy };
    case 'exposure':
      return { type: 'exposure', exposure: a.exposure, offset: a.offset, gamma: a.gamma };
    case 'vibrance':
      return { type: 'vibrance', vibrance: a.vibrance, saturation: a.saturation };
    case 'colorBalance':
      return { type: 'color balance', shadows: cb(a.shadows), midtones: cb(a.midtones), highlights: cb(a.highlights), preserveLuminosity: a.preserveLuminosity };
    case 'blackWhite':
      return { type: 'black & white', reds: a.weights[0], yellows: a.weights[1], greens: a.weights[2], cyans: a.weights[3], blues: a.weights[4], magentas: a.weights[5], useTint: a.tint, tintColor: psdColor(a.tintColor) };
    case 'gradientMap':
      return { type: 'gradient map', gradientType: 'solid', reverse: a.reverse, dither: a.dither, colorStops: a.stops.map((s) => ({ color: psdColor(s.color), location: s.pos, midpoint: 0.5 })), opacityStops: [{ opacity: 1, location: 0, midpoint: 0.5 }, { opacity: 1, location: 1, midpoint: 0.5 }] };
    case 'invert':
      return { type: 'invert' };
    case 'threshold':
      return { type: 'threshold', level: a.level };
  }
}

// ---- effects ----

function readEffects(e: LayerEffectsInfo, globalAngle: number, warn: Warn): LayerStyle | null {
  if (e.disabled) return null;
  const s: LayerStyle = {};
  const first = <T extends { enabled?: boolean }>(list: T[] | T | undefined) => (Array.isArray(list) ? list.find((x) => x.enabled !== false) ?? list[0] : list);
  const ds = first(e.dropShadow);
  if (ds)
    s.dropShadow = { enabled: ds.enabled !== false, blend: toBlend(ds.blendMode), color: rgb(ds.color), opacity: ds.opacity ?? 0.75, angle: ds.useGlobalLight ? globalAngle : (ds.angle ?? 120), distance: px(ds.distance, 5), spread: px(ds.choke, 0), size: px(ds.size, 5), knockout: ds.layerConceals ?? true };
  const is = first(e.innerShadow);
  if (is)
    s.innerShadow = { enabled: is.enabled !== false, blend: toBlend(is.blendMode), color: rgb(is.color), opacity: is.opacity ?? 0.75, angle: is.useGlobalLight ? globalAngle : (is.angle ?? 120), distance: px(is.distance, 5), spread: px(is.choke, 0), size: px(is.size, 5), knockout: false };
  const og = e.outerGlow;
  if (og) s.outerGlow = { enabled: og.enabled !== false, blend: toBlend(og.blendMode), color: rgb(og.color, [255, 255, 190]), opacity: og.opacity ?? 0.75, spread: px(og.choke, 0), size: px(og.size, 5), technique: 'softer' };
  const st = first(e.stroke);
  if (st) {
    if (st.fillType && st.fillType !== 'color') warn('strokeFill', { type: st.fillType });
    s.stroke = { enabled: st.enabled !== false, blend: toBlend(st.blendMode), color: rgb(st.color), opacity: st.opacity ?? 1, size: px(st.size, 3), position: st.position ?? 'outside' };
  }
  const sf = first(e.solidFill);
  if (sf) s.colorOverlay = { enabled: sf.enabled !== false, blend: toBlend(sf.blendMode), color: rgb(sf.color), opacity: sf.opacity ?? 1 };
  const go = first(e.gradientOverlay);
  if (go) {
    const grad = go.gradient && go.gradient.type === 'solid' ? go.gradient : null;
    const style = go.type === 'radial' || go.type === 'reflected' ? go.type : 'linear';
    if (go.type && go.type !== style) warn('gradientOverlay', { type: go.type });
    s.gradientOverlay = {
      enabled: go.enabled !== false,
      blend: toBlend(go.blendMode),
      opacity: go.opacity ?? 1,
      stops: grad ? grad.colorStops.map((c) => ({ pos: c.location, color: rgb(c.color) })) : [{ pos: 0, color: [0, 0, 0] }, { pos: 1, color: [255, 255, 255] }],
      style,
      angle: go.angle ?? 90,
      scale: go.scale ?? 100,
      reverse: !!go.reverse,
    };
  }
  for (const k of ['innerGlow', 'bevel', 'satin', 'patternOverlay'] as const) if (e[k] && (e[k] as { enabled?: boolean }).enabled !== false) warn(k);
  return Object.keys(s).length ? s : null;
}

function writeEffects(s: LayerStyle): LayerEffectsInfo {
  const e: LayerEffectsInfo = {};
  const v = (value: number) => ({ units: 'Pixels' as const, value });
  if (s.dropShadow) {
    const d = s.dropShadow;
    e.dropShadow = [{ enabled: d.enabled, present: true, showInDialog: true, blendMode: fromBlend(d.blend), color: psdColor(d.color), opacity: d.opacity, angle: d.angle, useGlobalLight: false, distance: v(d.distance), choke: v(d.spread), size: v(d.size), layerConceals: d.knockout }];
  }
  if (s.innerShadow) {
    const d = s.innerShadow;
    e.innerShadow = [{ enabled: d.enabled, present: true, showInDialog: true, blendMode: fromBlend(d.blend), color: psdColor(d.color), opacity: d.opacity, angle: d.angle, useGlobalLight: false, distance: v(d.distance), choke: v(d.spread), size: v(d.size) }];
  }
  if (s.outerGlow) {
    const g = s.outerGlow;
    e.outerGlow = { enabled: g.enabled, present: true, showInDialog: true, blendMode: fromBlend(g.blend), color: psdColor(g.color), opacity: g.opacity, choke: v(g.spread), size: v(g.size) };
  }
  if (s.stroke) {
    const k = s.stroke;
    e.stroke = [{ enabled: k.enabled, present: true, showInDialog: true, blendMode: fromBlend(k.blend), color: psdColor(k.color), opacity: k.opacity, size: v(k.size), position: k.position, fillType: 'color' }];
  }
  if (s.colorOverlay) {
    const c = s.colorOverlay;
    e.solidFill = [{ enabled: c.enabled, present: true, showInDialog: true, blendMode: fromBlend(c.blend), color: psdColor(c.color), opacity: c.opacity }];
  }
  if (s.gradientOverlay) {
    const g = s.gradientOverlay;
    e.gradientOverlay = [
      {
        enabled: g.enabled,
        present: true,
        showInDialog: true,
        blendMode: fromBlend(g.blend),
        opacity: g.opacity,
        type: g.style,
        angle: g.angle,
        scale: g.scale,
        reverse: g.reverse,
        align: true,
        gradient: { name: 'Custom', type: 'solid', colorStops: g.stops.map((c) => ({ color: psdColor(c.color), location: c.pos, midpoint: 0.5 })), opacityStops: [{ opacity: 1, location: 0, midpoint: 0.5 }, { opacity: 1, location: 1, midpoint: 0.5 }] },
      },
    ];
  }
  return e;
}

// ---- text ----

const FONT_FAMILIES: [RegExp, Family][] = [
  [/plex/i, 'ibmPlexSansArabic'],
  [/cairo/i, 'cairo'],
  [/tajawal/i, 'tajawal'],
  [/amiri/i, 'amiri'],
];
const WEIGHT_WORDS: [RegExp, number][] = [
  [/extra\s*light|ultra\s*light/i, 200],
  [/extra\s*bold|ultra\s*bold|heavy/i, 800],
  [/semi\s*bold|demi/i, 600],
  [/thin|hairline/i, 100],
  [/light/i, 300],
  [/medium/i, 500],
  [/black/i, 900],
  [/bold/i, 700],
];
const WEIGHT_NAMES: Record<number, string> = { 100: 'Thin', 200: 'ExtraLight', 300: 'Light', 400: 'Regular', 500: 'Medium', 600: 'SemiBold', 700: 'Bold', 800: 'ExtraBold', 900: 'Black', 1000: 'Black' };
const POSTSCRIPT: Record<Family, string> = { ibmPlexSansArabic: 'IBMPlexSansArabic', cairo: 'Cairo', tajawal: 'Tajawal', amiri: 'Amiri' };

function readText(t: LayerTextData, layerName: string, warn: Warn): TextData {
  const st = t.style ?? {};
  const fontName = st.font?.name ?? '';
  const family = FONT_FAMILIES.find(([re]) => re.test(fontName))?.[1];
  if (!family) warn('font', { name: fontName || '?' });
  if ((t.styleRuns?.length ?? 0) > 1) warn('mixedText', { layer: layerName });
  const weight = WEIGHT_WORDS.find(([re]) => re.test(fontName))?.[1] ?? 400;
  const m = t.transform ?? [1, 0, 0, 1, 0, 0];
  const strong = /[֐-ࣿ]/.exec(t.text) && !/^[^֐-ࣿ]*[A-Za-z]/.test(t.text);
  const just = t.paragraphStyle?.justification ?? 'left';
  const align = just.startsWith('justify') ? 'justify' : just === 'center' ? 'center' : (just === 'right') === !!strong ? 'start' : 'end';
  let matrix: TextData['matrix'] = [m[0], m[2], m[4], m[1], m[3], m[5]];
  let width: number | null = null;
  let height: number | null = null;
  if (t.shapeType === 'box' && t.boxBounds) {
    const [l, top, r, b] = t.boxBounds;
    width = r - l;
    height = b - top;
    matrix = [m[0], m[2], m[4] + m[0] * l + m[2] * top, m[1], m[3], m[5] + m[1] * l + m[3] * top];
  }
  return {
    text: t.text.replace(/\r/g, '\n'),
    style: {
      family: family ?? 'ibmPlexSansArabic',
      weight,
      italic: !!st.fauxItalic,
      size: st.fontSize ?? 24,
      color: [...rgb(st.fillColor), 255] as [number, number, number, number],
      lineHeight: st.autoLeading === false && st.leading ? st.leading : null,
      tracking: st.tracking ?? 0,
      align,
      direction: strong ? 'rtl' : 'auto',
      digits: st.hindiNumbers ? 'arabicIndic' : 'western',
      kashida: (st.kashida ?? 0) > 0 ? 'medium' : 'none',
    },
    width,
    height,
    matrix,
  };
}

function writeText(t: TextData): LayerTextData {
  const m = t.matrix;
  const s = t.style;
  const rtl = s.direction !== 'ltr';
  const just = s.align === 'justify' ? 'justify-all' : s.align === 'center' ? 'center' : (s.align === 'start') === rtl ? 'right' : 'left';
  return {
    text: t.text.replace(/\n/g, '\r'),
    transform: [m[0], m[3], m[1], m[4], m[2], m[5]],
    shapeType: t.width != null ? 'box' : 'point',
    ...(t.width != null ? { boxBounds: [0, 0, t.width, t.height ?? s.size * 1.2] } : {}),
    style: {
      font: { name: `${POSTSCRIPT[s.family]}-${WEIGHT_NAMES[s.weight] ?? 'Regular'}` },
      fontSize: s.size,
      fillColor: psdColor(s.color),
      tracking: s.tracking,
      autoLeading: s.lineHeight == null,
      ...(s.lineHeight != null ? { leading: s.lineHeight } : {}),
      hindiNumbers: s.digits === 'arabicIndic',
      kashida: s.kashida === 'none' ? 0 : 2,
      characterDirection: rtl ? 2 : 0,
    },
    paragraphStyle: { justification: just },
  } as LayerTextData;
}

// ---- shape layers ----

type UnitsValue = { units: string; value: number };
/** A length in document px (Photoshop keeps stroke widths in points). */
const length = (u: UnitsValue | undefined, ppi: number, d = 0) =>
  !u ? d : u.units === 'Points' ? (u.value * ppi) / 72 : u.units === 'Millimeters' ? (u.value * ppi) / 25.4 : u.units === 'Inches' ? u.value * ppi : u.value;
const unit01 = (v: number | undefined) => (v == null ? 1 : v > 1 ? v / 100 : v);

/** A Photoshop shape layer as an editable shape, when Mosamem can draw it as Photoshop does: a
 * solid fill (or none), an optional solid stroke, and paths that only add to each other. Anything
 * else stays pixels with a vector mask (the reader's other path). */
function readShape(l: Layer, ppi: number, color: (c: Color) => [number, number, number]): ShapeData | null {
  const vm = l.vectorMask;
  const vs = l.vectorStroke;
  const paths = vm?.paths?.filter((p) => p.knots.length > 0) ?? [];
  if (!vm || !paths.length || vm.invert || vm.disable || !l.vectorFill) return null;
  if (paths.some((p) => (p.operation ?? 'combine') !== 'combine') || (paths.length > 1 && paths.some((p) => p.fillRule === 'even-odd'))) return null;
  const fillOn = vs?.fillEnabled !== false;
  if (fillOn && l.vectorFill.type !== 'color') return null;
  let stroke: Stroke | null = null;
  if (vs?.strokeEnabled) {
    if (vs.content?.type !== 'color') return null;
    const width = length(vs.lineWidth, ppi, 1);
    stroke = {
      color: [...color(vs.content.color), Math.round(unit01(vs.opacity) * 255)],
      width,
      align: vs.lineAlignment ?? 'center',
      cap: vs.lineCapType ?? 'butt',
      join: vs.lineJoinType ?? 'miter',
      dash: (vs.lineDashSet ?? []).map((d) => d.value),
    };
  }
  const fill = fillOn && l.vectorFill.type === 'color' ? { kind: 'solid' as const, color: [...color(l.vectorFill.color), 255] as [number, number, number, number] } : null;
  const path: Path = {
    subs: paths.map((p) => ({
      closed: !p.open,
      knots: p.knots.map((k) => ({ i: [k.points[0], k.points[1]], p: [k.points[2], k.points[3]], o: [k.points[4], k.points[5]], smooth: k.linked })),
    })),
  };
  return { shape: liveShape(l, path) ?? { kind: 'free', path }, paint: { fill, stroke } };
}

/** A rectangle, rounded rectangle or ellipse Photoshop drew as one (and that wasn't turned since). */
function liveShape(l: Layer, path: Path): ShapeData['shape'] | null {
  const list = l.vectorOrigination?.keyDescriptorList;
  if (list?.length !== 1 || path.subs.length !== 1) return null;
  const k = list[0];
  const b = k.keyOriginShapeBoundingBox;
  if (!b || ![1, 2, 5].includes(k.keyOriginType ?? 0)) return null;
  const [x0, y0, x1, y1] = [b.left.value, b.top.value, b.right.value, b.bottom.value];
  // the outline must still fill that box: a turned or edited shape is a free path
  const ps = path.subs[0].knots.map((n) => n.p);
  const near = (a: number, c: number) => Math.abs(a - c) < 0.75;
  if (!near(Math.min(...ps.map((p) => p[0])), x0) || !near(Math.max(...ps.map((p) => p[0])), x1) || !near(Math.min(...ps.map((p) => p[1])), y0) || !near(Math.max(...ps.map((p) => p[1])), y1)) return null;
  if (k.keyOriginType === 5) return ps.length === 4 ? { kind: 'ellipse', x: x0, y: y0, w: x1 - x0, h: y1 - y0 } : null;
  const radii = k.keyOriginRRectRadii;
  const rs = radii ? [radii.topLeft, radii.topRight, radii.bottomLeft, radii.bottomRight].map((v) => v?.value ?? 0) : [0];
  if (rs.some((v) => Math.abs(v - rs[0]) > 0.01)) return null;
  if ((rs[0] > 0 ? 8 : 4) !== ps.length) return null;
  return { kind: 'rect', x: x0, y: y0, w: x1 - x0, h: y1 - y0, radius: rs[0] };
}

const unitsPx = (value: number) => ({ units: 'Pixels' as const, value });

/** A shape layer's outline, fill and stroke in Photoshop's terms (its pixels go along as well). */
function writeShape(l: Layer, s: ShapeData, ppi: number) {
  const path = shapePath(s.shape);
  l.vectorMask = {
    paths: path.subs.map((sub) => ({
      open: !sub.closed,
      operation: 'combine',
      fillRule: 'non-zero',
      knots: sub.knots.map((k) => ({ linked: k.smooth, points: [k.i[0], k.i[1], k.p[0], k.p[1], k.o[0], k.o[1]] })),
    })),
  };
  const f = s.paint.fill;
  const st = s.paint.stroke;
  const firstStop = f?.kind === 'linear' ? f.stops[0]?.color : null;
  l.vectorFill =
    f?.kind === 'linear'
      ? {
          type: 'solid',
          name: '',
          style: 'linear',
          angle: f.angle,
          colorStops: f.stops.map((g) => ({ color: psdColor(g.color), location: Math.round(g.pos * 4096), midpoint: 50 })),
          opacityStops: [{ opacity: 1, location: 0, midpoint: 50 }, { opacity: 1, location: 4096, midpoint: 50 }],
        }
      : { type: 'color', color: psdColor(f?.kind === 'solid' ? f.color : (firstStop ?? st?.color ?? [0, 0, 0])) };
  l.vectorStroke = {
    strokeEnabled: !!st,
    fillEnabled: !!f,
    lineWidth: unitsPx(st?.width ?? 1),
    lineDashOffset: unitsPx(0),
    miterLimit: 10,
    lineCapType: st?.cap ?? 'butt',
    lineJoinType: st?.join ?? 'miter',
    lineAlignment: st?.align ?? 'center',
    scaleLock: false,
    strokeAdjust: false,
    lineDashSet: (st?.dash ?? []).map((value) => ({ units: 'None' as const, value })),
    blendMode: 'normal',
    opacity: (st?.color[3] ?? 255) / 255,
    content: { type: 'color', color: psdColor(st?.color ?? [0, 0, 0]) },
    resolution: ppi,
  };
  // a rectangle or an ellipse stays live in Photoshop too
  const sh = s.shape;
  if (sh.kind === 'rect' || sh.kind === 'ellipse') {
    const [x0, y0, x1, y1] = [sh.x, sh.y, sh.x + sh.w, sh.y + sh.h];
    const r = sh.kind === 'rect' ? unitsPx(sh.radius) : unitsPx(0);
    l.vectorOrigination = {
      keyDescriptorList: [
        {
          keyOriginType: sh.kind === 'ellipse' ? 5 : sh.radius > 0 ? 2 : 1,
          keyOriginResolution: ppi,
          keyOriginShapeBoundingBox: { top: unitsPx(y0), left: unitsPx(x0), bottom: unitsPx(y1), right: unitsPx(x1) },
          ...(sh.kind === 'rect' ? { keyOriginRRectRadii: { topRight: r, topLeft: r, bottomLeft: r, bottomRight: r } } : {}),
          keyOriginBoxCorners: [{ x: x0, y: y0 }, { x: x1, y: y0 }, { x: x1, y: y1 }, { x: x0, y: y1 }],
        },
      ],
    };
  }
}

// ---- reading ----

// ---- vector masks and shapes ----

/** A vector mask's coverage over the canvas (0–255 per pixel), drawn antialiased on an offscreen
 * canvas: each path is filled on its own, then merged with the paths before it by its operation. */
function vectorCoverage(vm: LayerVectorMask, w: number, h: number): Uint8Array | null {
  if (typeof OffscreenCanvas === 'undefined' || !w || !h) return null;
  const acc = new OffscreenCanvas(w, h).getContext('2d');
  const one = new OffscreenCanvas(w, h).getContext('2d');
  if (!acc || !one) return null;
  acc.fillStyle = one.fillStyle = '#fff';
  // subtracting first takes the paths away from everything; any other first path starts from
  // nothing (Photoshop's own "starts with all pixels" flag is set on files whose composite
  // shows only the paths, so it isn't what decides)
  const first = vm.paths?.find((p) => !p.open && p.knots.length > 1);
  if (first?.operation === 'subtract') acc.fillRect(0, 0, w, h);
  const ops = { combine: 'source-over', subtract: 'destination-out', intersect: 'destination-in', exclude: 'xor' } as const;
  for (const path of vm.paths ?? []) {
    const k = path.knots;
    if (path.open || k.length < 2) continue;
    // knot points: [control in x, y, anchor x, y, control out x, y]
    one.globalCompositeOperation = 'copy';
    one.clearRect(0, 0, w, h);
    one.globalCompositeOperation = 'source-over';
    one.beginPath();
    one.moveTo(k[0].points[2], k[0].points[3]);
    for (let i = 1; i <= k.length; i++) {
      const a = k[i - 1].points;
      const b = k[i % k.length].points;
      one.bezierCurveTo(a[4], a[5], b[0], b[1], b[2], b[3]);
    }
    one.closePath();
    one.fill(path.fillRule === 'even-odd' ? 'evenodd' : 'nonzero');
    acc.globalCompositeOperation = ops[path.operation ?? 'combine'] ?? 'source-over';
    acc.drawImage(one.canvas, 0, 0);
  }
  if (vm.invert) {
    acc.globalCompositeOperation = 'xor';
    acc.fillRect(0, 0, w, h);
  }
  const rgba = acc.getImageData(0, 0, w, h).data;
  const out = new Uint8Array(w * h);
  for (let i = 0; i < out.length; i++) out[i] = rgba[i * 4 + 3];
  return out;
}

/** A mask channel spread over the whole canvas. */
function place(c: { data: Uint8Array; x: number; y: number; w: number; h: number }, outside: number, w: number, h: number): Uint8Array {
  const out = new Uint8Array(w * h).fill(outside);
  for (let y = Math.max(0, c.y); y < Math.min(h, c.y + c.h); y++) {
    for (let x = Math.max(0, c.x); x < Math.min(w, c.x + c.w); x++) out[y * w + x] = c.data[(y - c.y) * c.w + x - c.x];
  }
  return out;
}

/** One gray channel of a raster mask, with where it sits. */
function maskChannel(m: LayerMaskData, bpc: number): { data: Uint8Array; x: number; y: number; w: number; h: number } | null {
  const d = m.imageData;
  if (!d || !d.width || !d.height) return null;
  const src = bytes8(d.data as never, bpc, 1);
  const data = new Uint8Array(d.width * d.height);
  const step = src.length >= data.length * 4 ? 4 : 1;
  for (let i = 0; i < data.length; i++) data[i] = src[i * step];
  return { data, x: m.left ?? 0, y: m.top ?? 0, w: d.width, h: d.height };
}

/** The same file with an empty layer section, so only its flattened image is read. Sections, from
 * the public file format specification: header (26 bytes), color mode data, image resources, layer
 * and mask information (each led by its length; 4 bytes, or 8 for the last one in PSB), image data. */
function withoutLayers(buffer: ArrayBuffer): ArrayBuffer | null {
  const v = new DataView(buffer);
  if (buffer.byteLength < 34 || v.getUint32(0) !== 0x38425053) return null;
  const psb = v.getUint16(4) === 2;
  let at = 26;
  at += 4 + v.getUint32(at); // color mode data
  at += 4 + v.getUint32(at); // image resources
  if (at + (psb ? 8 : 4) > buffer.byteLength) return null;
  const layers = psb ? Number(v.getBigUint64(at)) : v.getUint32(at);
  const rest = at + (psb ? 8 : 4) + layers;
  if (rest > buffer.byteLength) return null;
  const head = new Uint8Array(buffer, 0, at);
  const tail = new Uint8Array(buffer, rest);
  const out = new Uint8Array(at + (psb ? 8 : 4) + tail.length);
  out.set(head, 0); // the zeroed length between them is the empty layer section
  out.set(tail, at + (psb ? 8 : 4));
  return out.buffer;
}

export function readPsdFile(buffer: ArrayBuffer): ReadResult {
  const warnings = new Map<string, PsdWarning>();
  const warn: Warn = (key, params) => warnings.set(JSON.stringify([key, params]), { key, params });
  const opts = { useImageData: true, skipThumbnail: true, skipLinkedFilesData: true };
  let psd: Psd;
  try {
    psd = readPsd(buffer, opts);
  } catch (e) {
    // a damaged layer section: open the file's flattened image instead
    const flat = withoutLayers(buffer);
    if (!flat) throw e;
    psd = readPsd(flat, opts);
    warn('flattened', { error: e instanceof Error ? e.message : String(e) });
  }
  const bpc = psd.bitsPerChannel ?? 8;
  const ppi = psd.imageResources?.resolutionInfo?.horizontalResolution ?? 72;
  const features = new Set<string>();
  const packer = new Packer();
  const pushPx = (data: { data: ArrayLike<number> & { buffer: ArrayBufferLike; byteOffset: number; byteLength: number }; width: number; height: number }, x: number, y: number): Pixels => ({
    offset: packer.push(bytes8(data.data, bpc)),
    x,
    y,
    w: data.width,
    h: data.height,
  });
  const globalAngle = psd.imageResources?.globalAngle ?? 120;
  const map = (l: Layer): PsdLayer | null => {
    if (l.sectionDivider && !l.children) return null;
    const node: PsdLayer = {
      name: l.name ?? '',
      kind: 'pixel',
      visible: !l.hidden,
      opacity: l.opacity ?? 1,
      fill: l.fillOpacity ?? 1,
      blend: toBlend(l.blendMode === 'pass through' ? 'normal' : l.blendMode),
      passThrough: l.blendMode === 'pass through',
      clipped: !!l.clipping,
      locks: { transparent: !!l.protected?.transparency, pixels: !!l.protected?.composite, position: !!l.protected?.position },
      expanded: l.opened ?? true,
      pixels: null,
      mask: null,
      children: [],
      adjustment: null,
      text: null,
      shape: null,
      style: null,
    };
    if (l.children) {
      node.kind = 'group';
      node.children = l.children.map(map).filter((c): c is PsdLayer => !!c);
      features.add('groups');
    } else if (l.adjustment) {
      const a = readAdjustment(l.adjustment, warn);
      if (a) {
        node.kind = 'adjustment';
        node.adjustment = a;
        features.add(`adjustment: ${a.kind}`);
      } else {
        node.visible = false;
        node.name = `${node.name} ⚠︎`;
      }
    } else {
      if (l.imageData) node.pixels = pushPx(l.imageData, l.left ?? 0, l.top ?? 0);
      if (l.text) {
        node.kind = 'text';
        node.text = readText(l.text, node.name, warn);
        features.add('text');
      }
      // an editable shape when it can be (no raster mask of the user's own beside it)
      const userMask = l.mask?.fromVectorData ? l.realMask : l.mask;
      const color32 = (c: Color) => rgb(c).map((v) => (bpc === 32 ? Math.round(srgb(v / 255) * 255) : v)) as [number, number, number];
      const shape = !l.text && !userMask ? readShape(l, ppi, color32) : null;
      if (shape) {
        node.kind = 'shape';
        node.shape = shape;
        // the pixels until the shape changes: Photoshop's own drawing when the file has it (pixels
        // already cut to the shape, or a fill with Photoshop's drawing of the outline); otherwise
        // the engine draws it
        if (l.mask?.fromVectorData) {
          node.pixels = null;
          const drawnMask = maskChannel(l.mask, bpc);
          const f = shape.paint.fill;
          if (drawnMask && f?.kind === 'solid' && !shape.paint.stroke) {
            const a = place(drawnMask, l.mask.defaultColor ?? 0, psd.width, psd.height);
            const px = new Uint8Array(a.length * 4);
            for (let i = 0; i < a.length; i++) (px[i * 4] = f.color[0]), (px[i * 4 + 1] = f.color[1]), (px[i * 4 + 2] = f.color[2]), (px[i * 4 + 3] = a[i]);
            node.pixels = { offset: packer.push(px), x: 0, y: 0, w: psd.width, h: psd.height };
          }
        }
        features.add(`shape layers: ${shape.shape.kind}`);
      }
      // a shape or fill layer saved without its pixels: fill the canvas with its color (its
      // vector mask, below, gives the shape)
      if (!shape && !node.pixels && l.vectorFill) {
        if (l.vectorFill.type === 'color') {
          // 32-bit documents keep fill colors in linear light, like their pixels
          const [r, g, b] = rgb(l.vectorFill.color).map((v) => (bpc === 32 ? Math.round(srgb(v / 255) * 255) : v));
          const px = new Uint8Array(psd.width * psd.height * 4);
          for (let i = 0; i < px.length; i += 4) (px[i] = r), (px[i + 1] = g), (px[i + 2] = b), (px[i + 3] = 255);
          node.pixels = { offset: packer.push(px), x: 0, y: 0, w: psd.width, h: psd.height };
          features.add('shape layers (as pixels)');
        } else warn('fillWithoutPixels');
      }
      if (!shape && l.vectorStroke?.strokeEnabled) warn('shapeStroke');
      if (l.placedLayer) warn('smartObject');
    }
    // Photoshop keeps the user's raster mask in `realMask` when the layer also has a vector mask;
    // a layer mask in Mosamem is one channel, so the two are multiplied into it
    const user = l.mask?.fromVectorData ? l.realMask : l.mask;
    // A shape layer saved with its pixels and without a drawing of its mask was already cut to
    // its shape (Photoshop composites those pixels as they are); otherwise the vector mask applies.
    const drawn = l.vectorMask && l.mask?.fromVectorData ? maskChannel(l.mask, bpc) : null;
    const cut = (!!l.vectorFill && !!l.imageData?.width && !drawn) || node.kind === 'shape';
    if (cut && l.vectorMask && node.kind !== 'shape') warn('vectorShape');
    const vm = l.vectorMask?.paths?.length && !cut ? l.vectorMask : null;
    // Photoshop also saves its own drawing of the vector mask (matching its edges exactly); our
    // drawing of the paths is for files without one
    const vector = vm ? (drawn ? place(drawn, l.mask?.defaultColor ?? 0, psd.width, psd.height) : vectorCoverage(vm, psd.width, psd.height)) : null;
    if (vm && !vector) warn('noCanvas');
    // ag-psd gives densities as 0–1 (older files may carry 0–255)
    const dens = (d: number | undefined) => (d == null ? 1 : d > 1 ? d / 255 : d);
    const uDensity = dens(l.mask?.userMaskDensity);
    const uFeather = l.mask?.userMaskFeather ?? 0;
    if (user && !vector) {
      const c = maskChannel(user, bpc);
      const pixels = c ? { offset: packer.push(c.data), x: c.x, y: c.y, w: c.w, h: c.h } : null;
      node.mask = { pixels, defaultColor: user.defaultColor ?? 0, enabled: !user.disabled, density: uDensity, feather: uFeather };
      features.add('layer masks');
    } else if (vector && vm) {
      const vDensity = dens(l.mask?.vectorMaskDensity);
      const soft = (v: number, d: number) => 255 - d * (255 - v);
      const W = psd.width;
      const out = new Uint8Array(vector.length);
      const c = user ? maskChannel(user, bpc) : null;
      const outside = user?.defaultColor ?? 255;
      const uOn = !!user && !user.disabled;
      // a disabled vector mask stays as a disabled mask, unless a working raster mask needs the slot
      const dropVector = !!vm.disable && uOn;
      for (let y = 0, i = 0; y < psd.height; y++) {
        for (let x = 0; x < W; x++, i++) {
          let v = dropVector ? 255 : soft(vector[i], vDensity);
          if (uOn) {
            const mx = x - (c?.x ?? 0);
            const my = y - (c?.y ?? 0);
            const u = c && mx >= 0 && my >= 0 && mx < c.w && my < c.h ? c.data[my * c.w + mx] : outside;
            v = (v * soft(u, uDensity)) / 255;
          }
          out[i] = Math.round(v);
        }
      }
      const vFeather = l.mask?.vectorMaskFeather ?? 0;
      node.mask = { pixels: { offset: packer.push(out), x: 0, y: 0, w: W, h: psd.height }, defaultColor: 0, enabled: !vm.disable || uOn, density: 1, feather: Math.max(uOn ? uFeather : 0, dropVector ? 0 : vFeather) };
      features.add('vector masks (as pixel masks)');
      if (dropVector) warn('vectorMaskDropped');
      if (user && !uOn) warn('rasterMaskDropped');
    }
    if (l.effects) {
      node.style = readEffects(l.effects, globalAngle, warn);
      if (node.style) features.add('layer styles');
    }
    if (node.clipped) features.add('clipping');
    if (node.blend !== 'normal') features.add('blend modes');
    if (node.fill < 1) features.add('fill opacity');
    return node;
  };
  const layers = (psd.children ?? []).map(map).filter((c): c is PsdLayer => !!c);
  // a flat file without layers: its composite is the one layer
  if (!layers.length && psd.imageData) {
    layers.push({ name: 'Background', kind: 'pixel', visible: true, opacity: 1, fill: 1, blend: 'normal', passThrough: false, clipped: false, locks: { transparent: false, pixels: false, position: false }, expanded: false, pixels: pushPx(psd.imageData as never, 0, 0), mask: null, children: [], adjustment: null, text: null, shape: null, style: null });
  }
  if (bpc !== 8) warn('depth', { bits: bpc });
  if (psd.colorMode !== undefined && psd.colorMode !== 3) warn('colorMode', { mode: psd.colorMode });
  // without "Maximize Compatibility" Photoshop stores a blank composite: nothing to compare with
  const merged = psd.imageResources?.versionInfo?.hasRealMergedData !== false;
  const composite = psd.imageData && merged ? pushPx(psd.imageData as never, 0, 0) : null;
  const doc: PsdDoc = { width: psd.width, height: psd.height, ppi, layers, composite };
  return { message: pack(doc, packer), composite, warnings: [...warnings.values()], features: [...features] };
}

// ---- writing ----

export function writePsdFile(message: Uint8Array): Uint8Array {
  const { doc, pixels } = unpack(message);
  const img = (p: Pixels | null, channels = 4) => {
    if (!p) return undefined;
    let data = pixels.subarray(p.offset, p.offset + p.w * p.h * channels);
    if (channels === 1) {
      // masks: one channel to gray RGBA
      const rgba = new Uint8ClampedArray(p.w * p.h * 4);
      for (let i = 0; i < p.w * p.h; i++) rgba[i * 4] = rgba[i * 4 + 1] = rgba[i * 4 + 2] = data[i], (rgba[i * 4 + 3] = 255);
      return { width: p.w, height: p.h, data: rgba };
    }
    data = new Uint8ClampedArray(data.buffer, data.byteOffset, data.byteLength) as unknown as Uint8Array;
    return { width: p.w, height: p.h, data: data as unknown as Uint8ClampedArray };
  };
  const map = (n: PsdLayer): Layer => {
    const l: Layer = {
      name: n.name,
      hidden: !n.visible,
      opacity: n.opacity,
      fillOpacity: n.fill,
      blendMode: n.kind === 'group' && n.passThrough ? 'pass through' : fromBlend(n.blend),
      clipping: n.clipped,
      protected: { transparency: n.locks.transparent, composite: n.locks.pixels, position: n.locks.position },
    };
    if (n.kind === 'group') {
      l.children = n.children.map(map);
      l.opened = n.expanded;
    } else if (n.kind === 'adjustment' && n.adjustment) {
      l.adjustment = writeAdjustment(n.adjustment);
    } else {
      const im = img(n.pixels);
      if (im && n.pixels) {
        l.left = n.pixels.x;
        l.top = n.pixels.y;
        l.imageData = im;
      }
      if (n.kind === 'text' && n.text) l.text = writeText(n.text);
      if (n.kind === 'shape' && n.shape) writeShape(l, n.shape, doc.ppi);
    }
    if (n.mask) {
      const im = img(n.mask.pixels, 1);
      l.mask = {
        defaultColor: n.mask.defaultColor,
        disabled: !n.mask.enabled,
        userMaskDensity: n.mask.density,
        userMaskFeather: n.mask.feather,
        ...(im && n.mask.pixels ? { left: n.mask.pixels.x, top: n.mask.pixels.y, right: n.mask.pixels.x + n.mask.pixels.w, bottom: n.mask.pixels.y + n.mask.pixels.h, imageData: im } : {}),
      };
    }
    if (n.style) l.effects = writeEffects(n.style);
    return l;
  };
  const psd: Psd = {
    width: doc.width,
    height: doc.height,
    children: doc.layers.map(map),
    imageData: img(doc.composite),
    imageResources: { resolutionInfo: { horizontalResolution: doc.ppi, horizontalResolutionUnit: 'PPI', widthUnit: 'Inches', verticalResolution: doc.ppi, verticalResolutionUnit: 'PPI', heightUnit: 'Inches' } },
  };
  return writePsdUint8Array(psd, { noBackground: true, invalidateTextLayers: false });
}
