// M5: the live preview's GLSL adjustments against the engine's CPU math, on a tile with every
// kind of color, partial alpha and partial coverage. Pass = every channel within ±1 of 255.
const { engine } = __h;
const c = window.__lawha.compositor;
const { decodeCompiledAdjustment } = await import('/src/engine/compositor.ts');
const lv = (o = {}) => ({ inBlack: 0, inWhite: 255, gamma: 1, outBlack: 0, outWhite: 255, ...o });
const hsl = (hue = 0, saturation = 0, lightness = 0) => ({ hue, saturation, lightness });
const cases = {
  levels: { kind: 'levels', channels: [lv({ inBlack: 20, inWhite: 230, gamma: 1.4, outBlack: 10, outWhite: 245 }), lv({ gamma: 0.8 }), lv(), lv({ outWhite: 200 })] },
  curves: { kind: 'curves', channels: [[[0, 0], [64, 48], [192, 215], [255, 255]], [[0, 0], [255, 255]], [[0, 20], [128, 140], [255, 255]], [[0, 0], [100, 80], [255, 230]]] },
  hueSaturation: { kind: 'hueSaturation', master: hsl(40, 30, -20), ranges: [hsl(), hsl(), hsl(-15, 40, 10), hsl(), hsl(20, -50, 0), hsl()], colorize: false, colorizeHsl: hsl(0, 25, 0) },
  colorize: { kind: 'hueSaturation', master: hsl(), ranges: [0, 1, 2, 3, 4, 5].map(() => hsl()), colorize: true, colorizeHsl: hsl(210, 60, 15) },
  brightnessContrast: { kind: 'brightnessContrast', brightness: 40, contrast: 30, legacy: false },
  legacyBC: { kind: 'brightnessContrast', brightness: -30, contrast: 50, legacy: true },
  exposure: { kind: 'exposure', exposure: 0.8, offset: 0.02, gamma: 1.2 },
  vibrance: { kind: 'vibrance', vibrance: 60, saturation: -20 },
  colorBalance: { kind: 'colorBalance', shadows: [20, 0, -30], midtones: [-40, 30, 10], highlights: [0, -20, 50], preserveLuminosity: false },
  colorBalanceLum: { kind: 'colorBalance', shadows: [20, 0, -30], midtones: [-40, 30, 10], highlights: [0, -20, 50], preserveLuminosity: true },
  blackWhite: { kind: 'blackWhite', weights: [40, 60, 40, 60, 20, 80], tint: false, tintColor: [225, 211, 179] },
  blackWhiteTint: { kind: 'blackWhite', weights: [120, -30, 70, 200, -100, 10], tint: true, tintColor: [225, 180, 120] },
  gradientMap: { kind: 'gradientMap', stops: [{ pos: 0, color: [20, 0, 60] }, { pos: 0.45, color: [200, 60, 40] }, { pos: 1, color: [255, 240, 180] }], reverse: false, dither: false },
  gradientMapDither: { kind: 'gradientMap', stops: [{ pos: 0, color: [0, 0, 0] }, { pos: 1, color: [255, 255, 255] }], reverse: true, dither: true },
  invert: { kind: 'invert' },
  threshold: { kind: 'threshold', level: 100 },
};
const out = { cases: 0, failed: [] };
for (const [name, settings] of Object.entries(cases)) {
  const op = decodeCompiledAdjustment(await engine.adjustmentCompile(settings));
  for (const mode of ['normal', 'multiply', 'color', 'dissolve']) {
    const ref = await engine.debugAdjustReference(settings, mode, 0.7);
    const n = 256 * 256 * 4;
    const [back, cover, cpu] = [ref.subarray(0, n), ref.subarray(n, 2 * n), ref.subarray(2 * n, 3 * n)];
    const gpu = c.adjustTest(back, cover, op, mode, 0.7);
    let worst = 0, at = -1;
    for (let i = 0; i < n; i++) {
      // fully transparent pixels have no color to compare
      if (cpu[i - (i % 4) + 3] === 0 && gpu[i - (i % 4) + 3] === 0) continue;
      const d = Math.abs(cpu[i] - gpu[i]);
      if (d > worst) { worst = d; at = i; }
    }
    out.cases++;
    if (worst > 1) {
      const p = at - (at % 4);
      out.failed.push({ name, mode, worst, px: [(p / 4) % 256, Math.floor(p / 4 / 256)], back: [...back.subarray(p, p + 4)], cpu: [...cpu.subarray(p, p + 4)], gpu: [...gpu.subarray(p, p + 4)] });
    }
  }
}
out.pass = out.failed.length === 0;
return out;
