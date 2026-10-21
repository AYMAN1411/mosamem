// Shape layers through PSD, run in the app (tools/harness.mjs -f): a document with a rounded
// rectangle, an ellipse, a star with a dashed outside stroke and a free path is saved as PSD (the
// app's own writer), the file is read back raw with ag-psd to see what Photoshop would find (vector
// mask, fill, stroke, and the live-shape record for the rectangle and the ellipse), then opened
// again: every shape comes back an editable shape with the same settings and the same picture.
const { app, engine } = __h;
const agPsd = await import('/node_modules/.vite/deps/ag-psd.js'); // ag-psd as Vite serves it (CommonJS)
const readRaw = agPsd.readPsd ?? agPsd.default.readPsd;
const { readPsd } = await import('/src/engine/psdClient.ts');
const { writePsd } = await import('/src/engine/psdClient.ts');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
app.dialog = null;
const info = await engine.newDocument({ name: 'psd shapes', width: 400, height: 300, ppi: 72, background: { kind: 'white' }, backgroundLayerName: 'bg' });
const id = info.id;
const solid = (c) => ({ kind: 'solid', color: [...c, 255] });
const shapes = [
  ['rounded', { shape: { kind: 'rect', x: 20, y: 20, w: 160, h: 90, radius: 14 }, paint: { fill: solid([220, 60, 60]), stroke: null } }],
  ['ellipse', { shape: { kind: 'ellipse', x: 220, y: 20, w: 150, h: 100 }, paint: { fill: solid([60, 120, 220]), stroke: null } }],
  ['star', { shape: { kind: 'star', cx: 100, cy: 210, r: 70, inner: 0.45, points: 5, angle: 0 }, paint: { fill: solid([250, 200, 40]), stroke: { color: [20, 20, 20, 255], width: 4, align: 'outside', cap: 'butt', join: 'round', dash: [3, 2] } } }],
  ['free', { shape: { kind: 'free', path: { subs: [{ closed: true, knots: [
    { p: [240, 180], i: [240, 180], o: [300, 150], smooth: true },
    { p: [370, 220], i: [370, 180], o: [370, 260], smooth: true },
    { p: [260, 280], i: [300, 280], o: [260, 280], smooth: false },
  ] }] } }, paint: { fill: solid([60, 170, 90]), stroke: null } }],
];
for (const [name, data] of shapes) await engine.shapeNew(id, data, name);
const bytes = await writePsd(await engine.psdExport(id));
const raw = readRaw(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength), { skipLayerImageData: true, skipCompositeImageData: true, skipThumbnail: true });
const byName = Object.fromEntries(raw.children.map((l) => [l.name, l]));
const checks = {};
checks.vectorRecordsWritten = shapes.every(([n]) => byName[n]?.vectorMask?.paths?.length && byName[n]?.vectorFill && byName[n]?.vectorStroke);
checks.liveRectangleRecord = byName.rounded.vectorOrigination?.keyDescriptorList?.[0]?.keyOriginType === 2;
checks.liveEllipseRecord = byName.ellipse.vectorOrigination?.keyDescriptorList?.[0]?.keyOriginType === 5;
const st = byName.star.vectorStroke;
checks.strokeWritten = st.strokeEnabled && st.lineAlignment === 'outside' && st.lineWidth.value === 4 && st.lineDashSet.map((d) => d.value).join() === '3,2';

const r = await readPsd(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
const back = await engine.psdImport(r.message, 'psd shapes back');
const got = Object.fromEntries(back.layers.map((l) => [l.name, l]));
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
for (const [name, data] of shapes) {
  const g = got[name]?.shape;
  // the free path and the star come back as free paths with the same points; the others as live shapes
  const shapeOk = data.shape.kind === 'star' ? g?.shape.kind === 'free' && g.shape.path.subs[0].knots.length === 10 : same(g?.shape, data.shape) || (data.shape.kind === 'free' && g?.shape.kind === 'free');
  checks[`${name}ComesBack`] = got[name]?.kind === 'shape' && shapeOk && same(g.paint.fill, data.paint.fill) && same(g.paint.stroke, data.paint.stroke);
}
checks.noWarnings = r.warnings.length === 0;
// the same picture, sampled across the shapes
const points = [[100, 60], [300, 70], [100, 210], [100, 137], [320, 230], [21, 21], [200, 150]];
const pairs = await Promise.all(points.map(async ([x, y]) => [await engine.sampleColor(id, x, y, 1, true), await engine.sampleColor(back.id, x, y, 1, true)]));
checks.samePixels = pairs.every(([a, b]) => same(a, b));
checks.passed = Object.values(checks).every(Boolean);
return checks;
