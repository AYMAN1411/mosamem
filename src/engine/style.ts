// Layer effects, mirroring lawha_core::style::LayerStyle.
import type { BlendMode } from './blend';
import type { GradientStop } from './adjust';

export interface Shadow { enabled: boolean; blend: BlendMode; color: [number, number, number]; opacity: number; angle: number; distance: number; spread: number; size: number; knockout: boolean }
export interface Glow { enabled: boolean; blend: BlendMode; color: [number, number, number]; opacity: number; spread: number; size: number; technique: 'softer' | 'precise' }
export interface Stroke { enabled: boolean; blend: BlendMode; color: [number, number, number]; opacity: number; size: number; position: 'outside' | 'inside' | 'center' }
export interface ColorOverlay { enabled: boolean; blend: BlendMode; color: [number, number, number]; opacity: number }
export interface GradientOverlay { enabled: boolean; blend: BlendMode; opacity: number; stops: GradientStop[]; style: 'linear' | 'radial' | 'reflected'; angle: number; scale: number; reverse: boolean }

export interface LayerStyle {
  dropShadow?: Shadow;
  innerShadow?: Shadow;
  outerGlow?: Glow;
  stroke?: Stroke;
  colorOverlay?: ColorOverlay;
  gradientOverlay?: GradientOverlay;
}

export type EffectKind = keyof LayerStyle;

/** In the order the dialog lists them. */
export const EFFECTS: EffectKind[] = ['dropShadow', 'innerShadow', 'outerGlow', 'stroke', 'colorOverlay', 'gradientOverlay'];

export function defaultEffect<K extends EffectKind>(kind: K): NonNullable<LayerStyle[K]> {
  const e: Required<LayerStyle> = {
    dropShadow: { enabled: true, blend: 'multiply', color: [0, 0, 0], opacity: 0.5, angle: 135, distance: 10, spread: 0, size: 10, knockout: true },
    innerShadow: { enabled: true, blend: 'multiply', color: [0, 0, 0], opacity: 0.5, angle: 135, distance: 5, spread: 0, size: 5, knockout: false },
    outerGlow: { enabled: true, blend: 'screen', color: [255, 255, 190], opacity: 0.75, spread: 0, size: 15, technique: 'softer' },
    stroke: { enabled: true, blend: 'normal', color: [0, 0, 0], opacity: 1, size: 3, position: 'outside' },
    colorOverlay: { enabled: true, blend: 'normal', color: [230, 60, 60], opacity: 1 },
    gradientOverlay: { enabled: true, blend: 'normal', opacity: 1, stops: [{ pos: 0, color: [0, 0, 0] }, { pos: 1, color: [255, 255, 255] }], style: 'linear', angle: 90, scale: 100, reverse: false },
  };
  return e[kind] as NonNullable<LayerStyle[K]>;
}

export const hasEffects = (s: LayerStyle | null | undefined) => !!s && EFFECTS.some((k) => s[k]?.enabled);
