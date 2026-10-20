// M8: time from setting effects on one full layer of the 6000×4000, 30-layer document until the
// view is drawn, at fit and at 100%. Then the same view again after an edit elsewhere (cached mattes).
const { app, engine } = __h;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let doc = app.docs.find((d) => d.info.name.startsWith('perf 6000'));
if (!doc) {
  app.addDocument(await engine.debugTestDocument(6000, 4000, 30));
  doc = app.active;
} else app.activeId = doc.info.id;
await sleep(500);
const c = window.__lawha.compositor;
const settle = async () => { await sleep(60); for (let i = 0; i < 3000 && !c.settled(); i++) await sleep(5); };
const id = doc.info.active;
const style = (size) => ({
  dropShadow: { enabled: true, blend: 'multiply', color: [0, 0, 0], opacity: 0.6, angle: 135, distance: 25, spread: 0, size, knockout: true },
  stroke: { enabled: true, blend: 'normal', color: [255, 255, 255], opacity: 1, size: 8, position: 'outside' },
});
const out = {};
for (const [name, zoom] of [['fit', 'fit'], ['100pct', 1]]) {
  if (zoom === 'fit') app.fit();
  else app.zoomTo(zoom);
  await settle();
  let t0 = performance.now();
  app.update(await engine.layerStyleSet(doc.info.id, id, style(name === 'fit' ? 40 : 41), false));
  await settle();
  const first = Math.round(performance.now() - t0);
  // an edit to another layer: the effect mattes come from the cache
  const other = doc.info.layers[3].id;
  t0 = performance.now();
  app.update(await engine.layerSet(doc.info.id, other, { opacity: 0.9 }, false));
  await settle();
  out[name] = { styleChangeMs: first, otherLayerEditMs: Math.round(performance.now() - t0) };
  app.update(await engine.undo(doc.info.id));
  app.update(await engine.undo(doc.info.id));
}
app.fit();
return out;
