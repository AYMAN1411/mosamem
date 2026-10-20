// M5: how fast an adjustment layer's preview follows a slider on the 6000×4000, 30-layer document.
// cpu: a transient settings change → engine recomposites → visible tiles fetched and drawn (the
// fallback path). gpu: the live preview → settings compiled → the next frame drawn.
const { app, actions, engine, live } = __h;
let c = window.__lawha.compositor;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const settle = async () => { for (let i = 0; i < 1000 && !c.settled(); i++) await sleep(5); };
const stats = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const q = (p) => +s[Math.min(s.length - 1, Math.floor(p * s.length))].toFixed(1);
  return { n: s.length, p50: q(0.5), p95: q(0.95), max: q(1) };
};
let doc = app.docs.find((d) => d.info.name.startsWith('perf 6000'));
if (!doc) {
  app.addDocument(await engine.debugTestDocument(6000, 4000, 30));
  doc = app.active;
} else app.activeId = doc.info.id;
// the canvas (and its compositor) may be new
await sleep(500);
c = window.__lawha.compositor;
await actions.adjust.add('curves');
const id = app.active.info.active;
const out = {};
for (const [name, zoom] of [['fit', 'fit'], ['100pct', 1]]) {
  if (zoom === 'fit') app.fit();
  else app.zoomTo(zoom);
  await settle();
  const times = [];
  for (let k = 0; k < 20; k++) {
    const y = 128 + 40 * Math.sin(k / 3);
    const t0 = performance.now();
    await engine.adjustmentSet(doc.info.id, id, { kind: 'curves', channels: [[[0, 0], [128, y], [255, 255]], [[0, 0], [255, 255]], [[0, 0], [255, 255]], [[0, 0], [255, 255]]] }, true).then((i) => app.update(i));
    await settle();
    times.push(performance.now() - t0);
  }
  out[name + '_cpu'] = stats(times);
  // the GPU path: start the session (first tick builds the cached parts), then time each tick
  const gpu = [];
  let first = 0;
  for (let k = 0; k < 40; k++) {
    const y = 128 + 40 * Math.sin(k / 3);
    const t0 = performance.now();
    const ok = await live.liveAdjust(id, { kind: 'curves', channels: [[[0, 0], [128, y], [255, 255]], [[0, 0], [255, 255]], [[0, 0], [255, 255]], [[0, 0], [255, 255]]] });
    if (!ok) throw new Error('no live preview');
    await new Promise((r) => { const prev = c.onFrame; c.onFrame = (ms) => { c.onFrame = prev; prev?.(ms); r(); }; });
    if (k === 0) { for (let i = 0; i < 2000 && c.pending() > 0; i++) await sleep(5); first = performance.now() - t0; } else gpu.push(performance.now() - t0);
  }
  await live.liveAdjustCommit(id, { kind: 'curves', channels: [[[0, 0], [128, 150], [255, 255]], [[0, 0], [255, 255]], [[0, 0], [255, 255]], [[0, 0], [255, 255]]] });
  await settle();
  out[name + '_gpu'] = { firstTickWithPartsMs: Math.round(first), ...stats(gpu) };
}
await actions.layer.delete();
app.fit();
return out;
