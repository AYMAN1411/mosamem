// M5: time to apply each filter to one full 6000×4000 layer of the 30-layer test document (each is
// undone afterwards).
const { app, engine } = __h;
let doc = app.docs.find((d) => d.info.name.startsWith('perf 6000'));
if (!doc) {
  app.addDocument(await engine.debugTestDocument(6000, 4000, 30));
  doc = app.active;
} else app.activeId = doc.info.id;
const id = doc.info.active;
const cases = [
  { kind: 'gaussianBlur', radius: 4 },
  { kind: 'gaussianBlur', radius: 50 },
  { kind: 'gaussianBlur', radius: 250 },
  { kind: 'motionBlur', angle: 30, distance: 40 },
  { kind: 'unsharpMask', amount: 120, radius: 3, threshold: 2 },
  { kind: 'addNoise', amount: 12, gaussian: true, monochromatic: false, seed: 5 },
  { kind: 'pixelate', cell: 24 },
];
const out = [];
for (const f of cases) {
  const t0 = performance.now();
  app.update(await engine.filterApply(doc.info.id, id, f, 'pixels', f.kind));
  out.push({ ...f, ms: Math.round(performance.now() - t0) });
  app.update(await engine.undo(doc.info.id));
}
return out;
