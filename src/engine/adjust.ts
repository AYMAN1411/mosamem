// Adjustment settings, mirroring `mosamem_core::adjust::Adjustment` (serde: `kind` tag, camelCase).

export interface Levels {
  inBlack: number;
  inWhite: number;
  gamma: number;
  outBlack: number;
  outWhite: number;
}

export interface Hsl {
  hue: number;
  saturation: number;
  lightness: number;
}

export interface GradientStop {
  pos: number;
  color: [number, number, number];
}

export type Adjustment =
  | { kind: 'levels'; channels: [Levels, Levels, Levels, Levels] }
  | { kind: 'curves'; channels: [number, number][][] }
  | { kind: 'hueSaturation'; master: Hsl; ranges: Hsl[]; colorize: boolean; colorizeHsl: Hsl }
  | { kind: 'brightnessContrast'; brightness: number; contrast: number; legacy: boolean }
  | { kind: 'exposure'; exposure: number; offset: number; gamma: number }
  | { kind: 'vibrance'; vibrance: number; saturation: number }
  | { kind: 'colorBalance'; shadows: number[]; midtones: number[]; highlights: number[]; preserveLuminosity: boolean }
  | { kind: 'blackWhite'; weights: number[]; tint: boolean; tintColor: [number, number, number] }
  | { kind: 'gradientMap'; stops: GradientStop[]; reverse: boolean; dither: boolean }
  | { kind: 'invert' }
  | { kind: 'threshold'; level: number };

export type AdjustmentKind = Adjustment['kind'];

/** In the order the Adjustments panel shows them. */
export const ADJUSTMENT_KINDS: AdjustmentKind[] = [
  'brightnessContrast',
  'levels',
  'curves',
  'exposure',
  'vibrance',
  'hueSaturation',
  'colorBalance',
  'blackWhite',
  'invert',
  'threshold',
  'gradientMap',
];

const levels = (): Levels => ({ inBlack: 0, inWhite: 255, gamma: 1, outBlack: 0, outWhite: 255 });
const hsl = (): Hsl => ({ hue: 0, saturation: 0, lightness: 0 });

/** Default settings: the same as `Adjustment::default_for` in the engine. */
export function defaultAdjustment(kind: AdjustmentKind): Adjustment {
  switch (kind) {
    case 'levels':
      return { kind, channels: [levels(), levels(), levels(), levels()] };
    case 'curves':
      return { kind, channels: [0, 1, 2, 3].map(() => [[0, 0], [255, 255]] as [number, number][]) };
    case 'hueSaturation':
      return { kind, master: hsl(), ranges: [0, 1, 2, 3, 4, 5].map(hsl), colorize: false, colorizeHsl: { hue: 0, saturation: 25, lightness: 0 } };
    case 'brightnessContrast':
      return { kind, brightness: 0, contrast: 0, legacy: false };
    case 'exposure':
      return { kind, exposure: 0, offset: 0, gamma: 1 };
    case 'vibrance':
      return { kind, vibrance: 0, saturation: 0 };
    case 'colorBalance':
      return { kind, shadows: [0, 0, 0], midtones: [0, 0, 0], highlights: [0, 0, 0], preserveLuminosity: true };
    case 'blackWhite':
      return { kind, weights: [40, 60, 40, 60, 20, 80], tint: false, tintColor: [225, 211, 179] };
    case 'gradientMap':
      return { kind, stops: [{ pos: 0, color: [0, 0, 0] }, { pos: 1, color: [255, 255, 255] }], reverse: false, dither: false };
    case 'invert':
      return { kind };
    case 'threshold':
      return { kind, level: 128 };
  }
}

export interface Histogram {
  red: number[];
  green: number[];
  blue: number[];
  luminosity: number[];
}
