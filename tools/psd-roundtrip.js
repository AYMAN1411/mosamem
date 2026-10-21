// M9: the PSD round trip, run inside the app (tools/harness.mjs). For each file in FILES:
// 1. open it (ag-psd reader → engine), render it, compare with the composite the file stores;
// 2. save it as PSD again, re-open that, compare structure and rendering with the first open.
const { app, engine, actions } = __h;
// from the dev server's sources, or (in a harness build, e.g. on Windows) from the test hooks
const { unpack } = __h.psdCodec ?? (await import('/src/engine/psd.ts'));
const { readPsd, writePsd } = __h.psd ?? (await import('/src/engine/psdClient.ts'));
const FILES = __FILES__;

function flatOf(buf) {
  const v = new DataView(buf);
  return { w: v.getUint32(0, true), h: v.getUint32(4, true), px: new Uint8Array(buf, 8) };
}
// RGB compared after compositing both over white, so colors under transparency don't count
function compare(a, b, w, h) {
  let max = 0, sum = 0, within1 = 0, within8 = 0, n = w * h;
  for (let i = 0; i < n; i++) {
    let worst = 0;
    for (let c = 0; c < 3; c++) {
      const va = a[i * 4 + c] * a[i * 4 + 3] / 255 + 255 - a[i * 4 + 3];
      const vb = b[i * 4 + c] * b[i * 4 + 3] / 255 + 255 - b[i * 4 + 3];
      const d = Math.abs(va - vb);
      worst = Math.max(worst, d);
      sum += d;
    }
    max = Math.max(max, worst);
    if (worst <= 1.0001) within1++;
    if (worst <= 8) within8++;
  }
  return { max: Math.round(max), mean: +(sum / (n * 3)).toFixed(2), within1: +(100 * within1 / n).toFixed(2), within8: +(100 * within8 / n).toFixed(2) };
}
function structure(layers, depth = 0, out = []) {
  for (const l of layers) {
    out.push([depth, l.name, l.kind, l.blend, l.passThrough, Math.round(l.opacity * 100), Math.round(l.fill * 100), l.visible, l.clipped, !!l.mask, l.adjustment?.kind ?? null, !!l.style, !!l.text]);
    structure(l.children ?? [], depth + 1, out);
  }
  return out;
}
const results = [];
for (const path of FILES) {
  const name = path.split(/[\\/]/).pop();
  const r = { file: name };
  try {
    const t0 = performance.now();
    const read = await readPsd(await engine.readFile(path));
    const first = unpack(read.message);
    r.size = `${first.doc.width}×${first.doc.height}`;
    r.features = read.features;
    r.warnings = read.warnings;
    const info = await engine.psdImport(read.message, name);
    r.openMs = Math.round(performance.now() - t0);
    const flat = flatOf(await engine.debugFlatten(info.id));
    if (first.doc.composite) {
      const c = first.doc.composite;
      const stored = first.pixels.subarray(c.offset, c.offset + c.w * c.h * 4);
      r.vsPhotoshop = c.w === flat.w && c.h === flat.h ? compare(flat.px, stored, flat.w, flat.h) : 'size differs';
    } else r.vsPhotoshop = 'no composite in file';
    // save as PSD and open again
    const out = await writePsd(await engine.psdExport(info.id));
    const again = await readPsd(out.buffer);
    const second = unpack(again.message);
    const s1 = JSON.stringify(structure(first.doc.layers));
    const s2 = JSON.stringify(structure(second.doc.layers));
    r.structureKept = s1 === s2;
    if (!r.structureKept) r.structureDiff = { before: structure(first.doc.layers).slice(0, 6), after: structure(second.doc.layers).slice(0, 6) };
    const info2 = await engine.psdImport(again.message, name);
    const flat2 = flatOf(await engine.debugFlatten(info2.id));
    r.reopened = compare(flat.px, flat2.px, flat.w, flat.h);
    await engine.closeDocument(info.id);
    await engine.closeDocument(info2.id);
  } catch (e) {
    r.error = String(e && e.message || e);
  }
  results.push(r);
}
return results;
