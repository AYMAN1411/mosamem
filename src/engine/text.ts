// Text settings, mirroring mosamem_core::text::TextStyle and the engine's TextData.

export type Family = 'ibmPlexSansArabic' | 'cairo' | 'tajawal' | 'amiri';
export type Align = 'start' | 'center' | 'end' | 'justify';
export type TextDirection = 'rtl' | 'ltr' | 'auto';
export type Digits = 'western' | 'arabicIndic' | 'persian';
export type Kashida = 'none' | 'short' | 'medium' | 'long';

export interface TextStyle {
  family: Family;
  weight: number;
  italic: boolean;
  size: number;
  color: [number, number, number, number];
  lineHeight: number | null;
  /** 1/1000 em */
  tracking: number;
  align: Align;
  direction: TextDirection;
  digits: Digits;
  kashida: Kashida;
}

export interface TextData {
  text: string;
  style: TextStyle;
  width: number | null;
  height: number | null;
  /** Layout px → document px, row-major 2×3. */
  matrix: [number, number, number, number, number, number];
  /** Opened from another program's file: its pixels show until the text is edited. */
  keepPixels?: boolean;
}

export const FAMILIES: { id: Family; css: string; weights: number[]; italic: boolean }[] = [
  { id: 'ibmPlexSansArabic', css: 'IBM Plex Sans Arabic', weights: [100, 200, 300, 400, 500, 600, 700], italic: false },
  { id: 'cairo', css: 'Cairo', weights: [200, 300, 400, 500, 600, 700, 800, 900, 1000], italic: false },
  { id: 'tajawal', css: 'Tajawal', weights: [200, 300, 400, 500, 700, 800, 900], italic: false },
  { id: 'amiri', css: 'Amiri', weights: [400, 700], italic: true },
];

export const defaultTextStyle = (): TextStyle => ({
  family: 'ibmPlexSansArabic',
  weight: 400,
  italic: false,
  size: 72,
  color: [0, 0, 0, 255],
  lineHeight: null,
  tracking: 0,
  align: 'start',
  direction: 'rtl',
  digits: 'western',
  kashida: 'none',
});
