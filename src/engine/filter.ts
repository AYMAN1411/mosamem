// Filter settings, mirroring `mosamem_core::filter::Filter` (serde: `kind` tag, camelCase).

export type Filter =
  | { kind: 'gaussianBlur'; radius: number }
  | { kind: 'motionBlur'; angle: number; distance: number }
  | { kind: 'unsharpMask'; amount: number; radius: number; threshold: number }
  | { kind: 'addNoise'; amount: number; gaussian: boolean; monochromatic: boolean; seed: number }
  | { kind: 'pixelate'; cell: number };

export type FilterKind = Filter['kind'];

export const newSeed = () => Math.floor(Math.random() * 0xffffffff) >>> 0;

export function defaultFilter(kind: FilterKind): Filter {
  switch (kind) {
    case 'gaussianBlur':
      return { kind, radius: 4 };
    case 'motionBlur':
      return { kind, angle: 0, distance: 20 };
    case 'unsharpMask':
      return { kind, amount: 100, radius: 2, threshold: 0 };
    case 'addNoise':
      return { kind, amount: 10, gaussian: false, monochromatic: false, seed: newSeed() };
    case 'pixelate':
      return { kind, cell: 8 };
  }
}

/** Filter menu groups, in order. */
export const FILTER_MENU: { group: 'blur' | 'sharpen' | 'noise' | 'pixelate'; kinds: FilterKind[] }[] = [
  { group: 'blur', kinds: ['gaussianBlur', 'motionBlur'] },
  { group: 'sharpen', kinds: ['unsharpMask'] },
  { group: 'noise', kinds: ['addNoise'] },
  { group: 'pixelate', kinds: ['pixelate'] },
];
