// M4 performance gate: paint latency with a 500 px brush on the 6000×4000, 30-layer document.
// Latency = pointer event → engine paints → changed tiles fetched → frame drawn.
const { app, engine } = __h;
const c = window.__mosamem.compositor;
const pt = window.__mosamem.paintTool;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const stats = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const q = (p) => +s[Math.min(s.length - 1, Math.floor(p * s.length))].toFixed(1);
  return { n: s.length, p50: q(0.5), p95: q(0.95), p99: q(0.99), max: q(1) };
};
let doc = app.docs.find((d) => d.info.name.startsWith('perf 6000'));
if (!doc) {
  app.addDocument(await engine.debugTestDocument(6000, 4000, 30));
  doc = app.active;
} else app.activeId = doc.info.id;
const out = {};
app.tool = 'brush';
Object.assign(app.paint.brush, { diameter: 500, hardness: 0.5, opacity: 1, flow: 1, spacing: 0.1, smoothing: 0, pressureSize: true });
const stage = document.querySelector('.stage');
const r = stage.getBoundingClientRect();
const ev = (type, x, y, p) => new PointerEvent(type, { bubbles: true, clientX: r.left + x, clientY: r.top + y, button: 0, buttons: 1, pointerId: 1, pointerType: 'pen', pressure: p });

async function stroke(name, zoom) {
  if (zoom === 'fit') app.fit();
  else app.zoomTo(zoom);
  for (let i = 0; i < 400 && !c.settled(); i++) await sleep(20);
  pt.latencies = [];
  const W = app.viewport.w, H = app.viewport.h;
  stage.dispatchEvent(ev('pointerdown', W * 0.2, H * 0.3, 0.2));
  const t0 = performance.now();
  // a 1.5 s wavy stroke, pen samples every 8 ms (120 Hz), pressure ramping 0.2 → 1
  for (let k = 1; k <= 190; k++) {
    const u = k / 190;
    stage.dispatchEvent(ev('pointermove', W * (0.2 + 0.6 * u), H * (0.5 + 0.2 * Math.sin(u * 9)), 0.2 + 0.8 * u));
    await sleep(8);
  }
  stage.dispatchEvent(ev('pointerup', W * 0.8, H * 0.5, 1));
  await sleep(400);
  for (let i = 0; i < 400 && !c.settled(); i++) await sleep(20);
  out[name] = { latencyMs: stats(pt.latencies), strokeMs: Math.round(performance.now() - t0), history: app.active.info.history.labels.at(-1)?.key };
  pt.latencies = null;
}
// the first stroke on a layer builds the cached below/above parts; later strokes reuse them
await stroke('fit_firstStroke', 'fit');
await stroke('fit_nextStroke', 'fit');
await stroke('100pct_firstStroke', 1);
await stroke('100pct_nextStroke', 1);
app.fit();
return out;
