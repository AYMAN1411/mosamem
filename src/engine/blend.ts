// Blend modes, in the same order as lawha-core's BlendMode::ALL (the GPU uses this index).
export const BLEND_MODES = [
  'normal', 'dissolve',
  'darken', 'multiply', 'colorBurn', 'linearBurn', 'darkerColor',
  'lighten', 'screen', 'colorDodge', 'linearDodge', 'lighterColor',
  'overlay', 'softLight', 'hardLight', 'vividLight', 'linearLight', 'pinLight', 'hardMix',
  'difference', 'exclusion', 'subtract', 'divide',
  'hue', 'saturation', 'color', 'luminosity',
] as const;

export type BlendMode = (typeof BLEND_MODES)[number];
