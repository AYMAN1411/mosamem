// M6: engine time for transforms and resizes on the 6000×4000, 30-layer test document (each step
// is undone afterwards).
const { app, engine } = __h;
let doc = app.docs.find((d) => d.info.name.startsWith('perf 6000'));
if (!doc) {
  app.addDocument(await engine.debugTestDocument(6000, 4000, 30));
  doc = app.active;
} else app.activeId = doc.info.id;
const id = doc.info.id;
const top = doc.info.active;
const rot = (deg, s, cx, cy) => {
  const r = (deg * Math.PI) / 180;
  const c = Math.cos(r) * s;
  const n = Math.sin(r) * s;
  return [c, -n, cx - c * cx + n * cy, n, c, cy - n * cx - c * cy, 0, 0, 1];
};
const steps = [
  ['move one layer by whole pixels', () => engine.layerTransform(id, [top], [1, 0, 37, 0, 1, -21, 0, 0, 1], 'bicubic', 'history.movePixels')],
  ['rotate 15° and scale 90%, one layer, bicubic', () => engine.layerTransform(id, [top], rot(15, 0.9, 3000, 2000), 'bicubic', 'history.transform')],
  ['same, Lanczos', () => engine.layerTransform(id, [top], rot(15, 0.9, 3000, 2000), 'lanczos', 'history.transform')],
  ['image size to 3000×2000, all 30 layers, bicubic', () => engine.imageSize(id, 3000, 2000, 72, 'bicubic')],
  ['canvas size to 6400×4400, all layers', () => engine.canvasSize(id, 6400, 4400, [0.5, 0.5], null)],
  ['crop 5000×3000 with a 3° straighten, all layers', () => engine.crop(id, 500, 500, 5000, 3000, 3, true, 'bicubic')],
  ['rotate the image 90°, all layers', () => engine.turnImage(id, 'cw90')],
];
const out = [];
for (const [name, f] of steps) {
  const t0 = performance.now();
  app.update(await f());
  out.push({ step: name, ms: Math.round(performance.now() - t0) });
  app.update(await engine.undo(id));
}
return out;
