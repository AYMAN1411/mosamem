// M6: the live transform preview on the 6000×4000, 30-layer document: time to start (cached parts),
// then the time from a change of the transform to the drawn frame.
const { app, engine, transform } = __h;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let doc = app.docs.find((d) => d.info.name.startsWith('perf 6000'));
if (!doc) {
  app.addDocument(await engine.debugTestDocument(6000, 4000, 30));
  doc = app.active;
} else app.activeId = doc.info.id;
await sleep(400);
app.fit();
const c = window.__mosamem.compositor;
for (let i = 0; i < 400 && !c.settled(); i++) await sleep(20);
const t0 = performance.now();
await transform.begin(true);
for (let i = 0; i < 2000 && c.pending() > 0; i++) await sleep(5);
const startMs = performance.now() - t0;
const frame = () => new Promise((r) => { const prev = c.onFrame; c.onFrame = (ms) => { c.onFrame = prev; prev?.(ms); r(); }; });
const times = [];
for (let k = 0; k < 40; k++) {
  const t = performance.now();
  transform.setNumbers({ angle: k * 2, w: 100 - k, h: 100 - k / 2 });
  await frame();
  times.push(performance.now() - t);
}
transform.cancel();
times.sort((a, b) => a - b);
return { startMs: Math.round(startMs), p50: +times[20].toFixed(1), p95: +times[38].toFixed(1), max: +times[39].toFixed(1) };
