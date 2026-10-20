// M1 performance gate: a 6000×4000 document with 30 layers must pan and zoom smoothly.
// Run with: node tools/harness.mjs -f tools/perf-m1.js   (window in front: tools/front.sh on)
const { app, engine } = __h;
const c = window.__lawha.compositor;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const raf = () => new Promise((r) => requestAnimationFrame(r));
const stats = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const q = (p) => +s[Math.min(s.length - 1, Math.floor(p * s.length))].toFixed(2);
  return { n: s.length, p50: q(0.5), p95: q(0.95), p99: q(0.99), max: q(1) };
};
async function settle(limit = 8000) {
  const t0 = performance.now();
  await raf();
  while (!c.settled() && performance.now() - t0 < limit) await sleep(10);
  return Math.round(performance.now() - t0);
}

const out = {};
const existing = app.docs.find((d) => d.info.name.startsWith('perf 6000'));
if (existing) {
  app.activeId = existing.info.id;
} else {
  const t = performance.now();
  const info = await engine.debugTestDocument(6000, 4000, 30);
  out.createMs = Math.round(performance.now() - t);
  app.addDocument(info);
}
out.firstFullDetailMs = await settle();

// refresh baseline
const idle = [];
let last = await raf();
for (let i = 0; i < 60; i++) { const n = await raf(); idle.push(n - last); last = n; }
out.refresh = stats(idle);
const refresh = out.refresh.p50;

async function scenario(name, ms, step) {
  const intervals = [];
  const draws = [];
  let blurry = 0;
  c.onFrame = (d) => draws.push(d);
  const t0 = await raf();
  let prev = t0;
  for (;;) {
    const now = await raf();
    intervals.push(now - prev);
    prev = now;
    const u = (now - t0) / ms;
    if (u > 1) break;
    step(u);
    if (c.pending() > 0) blurry++;
  }
  c.onFrame = null;
  const sharpAfterMs = await settle();
  out[name] = {
    frameIntervalMs: stats(intervals),
    fps: +(1000 / (intervals.reduce((a, b) => a + b, 0) / intervals.length)).toFixed(1),
    droppedFrames: intervals.filter((x) => x > refresh * 1.5).length,
    gpuDrawMs: stats(draws),
    framesWhileTilesLoading: blurry,
    sharpAfterMs,
  };
}

app.fit();
await settle();
const fitZoom = app.active.view.zoom;
await scenario('pan_fit', 4000, (u) => { app.active.view = { ...app.active.view, cx: 3000 + 1500 * Math.sin(u * Math.PI * 2), cy: 2000 + 800 * Math.cos(u * Math.PI * 2) }; });
app.actualPixels();
await settle();
await scenario('pan_100pct', 5000, (u) => { app.active.view = { ...app.active.view, zoom: 1, cx: 800 + 4400 * u, cy: 2000 + 1200 * Math.sin(u * Math.PI * 2) }; });
await scenario('zoom_sweep_fit_to_400pct', 5000, (u) => { app.active.view = { ...app.active.view, zoom: fitZoom * Math.pow(4 / fitZoom, 0.5 - 0.5 * Math.cos(u * Math.PI * 2)), cx: 3000, cy: 2000 }; });
await scenario('rotate_360_at_fit', 4000, (u) => { app.active.view = { ...app.active.view, zoom: fitZoom, cx: 3000, cy: 2000, rotation: u * 360 }; });
app.rotateTo(0);
return out;
