// GPU vs CPU: every blend mode through the live-preview GLSL against mosamem-core (±1/255 expected).
// Run: node tools/harness.mjs -f tools/gpu-blend-test.js
const { engine, bridge } = __h;
const c = __h.display.compositor;
const modes = ['normal', 'dissolve', 'darken', 'multiply', 'colorBurn', 'linearBurn', 'darkerColor', 'lighten', 'screen', 'colorDodge', 'linearDodge', 'lighterColor', 'overlay', 'softLight', 'hardLight', 'vividLight', 'linearLight', 'pinLight', 'hardMix', 'difference', 'exclusion', 'subtract', 'divide', 'hue', 'saturation', 'color', 'luminosity'];
const N = 256 * 256 * 4;
const out = [];
for (const mode of modes) {
  for (const opacity of [1, 0.6]) {
    const ref = await engine.debugBlendReference(mode, opacity);
    const back = ref.subarray(0, N), src = ref.subarray(N, 2 * N), cpu = ref.subarray(2 * N, 3 * N);
    const gpu = c.blendTest(back, src, mode, opacity);
    let max = 0, over1 = 0, px = 0;
    for (let i = 0; i < N; i += 4) {
      px++;
      // fully transparent pixels have no meaningful color
      if (cpu[i + 3] === 0 && gpu[i + 3] === 0) continue;
      let d = 0;
      for (let k = 0; k < 4; k++) d = Math.max(d, Math.abs(cpu[i + k] - gpu[i + k]));
      max = Math.max(max, d);
      if (d > 1) over1++;
    }
    out.push({ mode, opacity, maxDiff: max, pixelsOver1: over1, pctWithin1: +(100 * (1 - over1 / px)).toFixed(3) });
  }
}
return out;
