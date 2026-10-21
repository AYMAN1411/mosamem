// A tour of the main features through the engine, for a platform that hasn't run them before (the
// Windows build): run it in the app (tools/harness.mjs -f). Painting, layers, an adjustment
// layer, a filter, Arabic text, a shape, a selection with a mask, undo and redo, saving and opening
// .mosamem, exporting PNG/JPEG/WebP and opening them, and PSD out and in. Each step checks pixels.
const { app, engine, psd, text } = __h;
const win = navigator.userAgent.includes('Windows');
const dir = win ? 'C:\\Mosamem\\test' : '/private/tmp';
const file = (name) => (win ? `${dir}\\${name}` : `${dir}/${name}`);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const checks = {};
const step = async (name, f) => {
  try {
    checks[name] = !!(await f());
  } catch (e) {
    checks[name] = `error: ${e?.message ?? e}`;
  }
};
const color = (doc, x, y) => engine.sampleColor(doc, x, y, 1, true);
const near = (a, b, tol = 2) => !!a && !!b && a.every((v, i) => Math.abs(v - b[i]) <= tol);
app.dialog = null;

let info = await engine.newDocument({ name: 'tour', width: 600, height: 400, ppi: 72, background: { kind: 'white' }, backgroundLayerName: 'bg' });
app.addDocument(info);
const id = info.id;
const bg = info.active;
await sleep(300);

let paint;
await step('brushPaints', async () => {
  info = await engine.layerNew(id, 'paint');
  paint = info.active;
  const s = await engine.paintBegin(id, { tool: 'brush', target: 'pixels', layer: paint, color: [220, 20, 20], brush: { diameter: 40, hardness: 1, spacing: 0.1, flow: 1, opacity: 1, pressureSize: false, pressureOpacity: false, minSize: 0.1 }, offset: null, sampleAll: false });
  await engine.paintPoints(s.stroke, s.live, [[100, 100, 1], [300, 100, 1], [500, 100, 1]]);
  app.update(await engine.paintEnd(s.stroke));
  return near(await color(id, 300, 100), [220, 20, 20, 255]) && near(await color(id, 300, 160), [255, 255, 255, 255]);
});

await step('layersBlendOpacityGroup', async () => {
  info = await engine.layerDuplicate(id, [paint], ' copy');
  const dup = info.active;
  info = await engine.layerSet(id, dup, { blend: 'multiply', opacity: 0.5 });
  info = await engine.layerGroup(id, 'group', [dup]);
  app.update(info);
  const g = info.layers.find((l) => l.kind === 'group');
  return g?.children?.[0]?.blend === 'multiply' && Math.abs(g.children[0].opacity - 0.5) < 1e-6;
});

await step('undoRedo', async () => {
  const before = info.layers.length;
  app.update(await engine.undo(id));
  const undone = (await engine.documentInfo(id)).layers.some((l) => l.kind === 'group');
  app.update((info = await engine.redo(id)));
  return !undone && info.layers.length === before && info.layers.some((l) => l.kind === 'group');
});

await step('adjustmentLayerInverts', async () => {
  info = await engine.adjustmentNew(id, 'invert', { kind: 'invert' });
  const c = await color(id, 300, 300); // white → black
  app.update((info = await engine.layerDelete(id, [info.active])));
  return near(c, [0, 0, 0, 255]) && near(await color(id, 300, 300), [255, 255, 255, 255]);
});

await step('gaussianBlurSoftensTheEdge', async () => {
  const before = await color(id, 300, 121); // just outside the 40 px stroke (y 80–120)
  app.update((info = await engine.filterApply(id, paint, { kind: 'gaussianBlur', radius: 6 }, 'pixels', 'blur')));
  const after = await color(id, 300, 121);
  return before[1] > 250 && after[1] < 245 && after[0] > 200;
});

await step('arabicText', async () => {
  const style = { ...text.defaultTextStyle(), size: 48, color: [20, 60, 160, 255] };
  info = await engine.textNew(id, { text: 'مرحبا بالعالم', style, width: null, height: null, matrix: [1, 0, 300, 0, 1, 250] });
  app.update(info);
  const t = info.layers.find((l) => l.kind === 'text');
  const b = await engine.transformBounds(id, [t.id]);
  // some of the box is the text's blue
  let blue = 0;
  for (let x = b.x; x < b.x + b.w; x += 4) for (let y = b.y; y < b.y + b.h; y += 4) if ((await color(id, x, y))[2] > 120 && (await color(id, x, y))[0] < 100) blue++;
  return t.text.text === 'مرحبا بالعالم' && b.w > 100 && blue > 10;
});

await step('shapeLayer', async () => {
  info = await engine.shapeNew(id, { shape: { kind: 'star', cx: 100, cy: 300, r: 60, inner: 0.5, points: 5, angle: 0 }, paint: { fill: { kind: 'solid', color: [30, 160, 60, 255] }, stroke: null } }, 'star');
  app.update(info);
  return near(await color(id, 100, 300), [30, 160, 60, 255]);
});

await step('selectionAndMask', async () => {
  await engine.selectShape(id, { kind: 'rect', x0: 0, y0: 0, x1: 300, y1: 400 }, 'replace', 0);
  info = await engine.maskAdd(id, paint, 'revealSelection');
  info = await engine.selectSimple(id, 'deselect');
  // the stroke stays on the left half, the right half is hidden by the mask (the group holds a
  // copy of the stroke without the mask: hidden while looking)
  const group = info.layers.find((l) => l.kind === 'group').id;
  await engine.layerSet(id, group, { visible: false });
  const ok = near(await color(id, 150, 100), [220, 20, 20, 255], 30) && near(await color(id, 450, 100), [255, 255, 255, 255]);
  app.update((info = await engine.layerSet(id, group, { visible: true })));
  return ok;
});

const probes = [[150, 100], [450, 100], [100, 300], [300, 300], [560, 380]];
const probe = async (doc) => Promise.all(probes.map(([x, y]) => color(doc, x, y)));
const want = await probe(id);

await step('saveAndOpenMosamem', async () => {
  await engine.saveDocument(id, file('tour.mosamem'));
  const back = await engine.openFile(file('tour.mosamem'), 'Layer');
  const got = await probe(back.id);
  const kinds = JSON.stringify(back.layers.map((l) => l.kind));
  await engine.closeDocument(back.id);
  return got.every((c, i) => near(c, want[i], 0)) && kinds === JSON.stringify(info.layers.map((l) => l.kind));
});

for (const [format, tol] of [['png', 0], ['jpeg', 12], ['webp', 0]]) {
  await step(`export_${format}`, async () => {
    const path = file(`tour.${format === 'jpeg' ? 'jpg' : format}`);
    await engine.exportImage({ doc: id, path, format, quality: 90, lossless: format === 'webp', source: { kind: 'document' }, size: null });
    const bytes = await engine.readFile(path);
    const back = await engine.openFile(path, 'Layer');
    const got = await probe(back.id);
    await engine.closeDocument(back.id);
    return bytes.byteLength > 1000 && back.width === 600 && got.every((c, i) => near(c.slice(0, 3), want[i].slice(0, 3), tol));
  });
}

await step('psdOutAndIn', async () => {
  const bytes = await psd.writePsd(await engine.psdExport(id));
  await engine.writeFile(file('tour.psd'), bytes);
  const r = await psd.readPsd(await engine.readFile(file('tour.psd')));
  const back = await engine.psdImport(r.message, 'tour.psd');
  const got = await probe(back.id);
  const kinds = back.layers.map((l) => l.kind);
  await engine.closeDocument(back.id);
  return got.every((c, i) => near(c, want[i], 2)) && kinds.includes('text') && kinds.includes('shape') && kinds.includes('group');
});

checks.platform = navigator.userAgent.match(/Windows NT [\d.]+|Mac OS X [\d_]+/)?.[0] ?? navigator.userAgent;
checks.passed = Object.entries(checks).every(([k, v]) => k === 'platform' || v === true);
return checks;
