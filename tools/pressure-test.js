// Simulated pen pressure (no tablet here): a stroke whose pressure ramps 0.1 → 1 must get wider
// along its length, both for Pointer Events pens (Windows-style) and for the native macOS bridge
// (mouse events plus forwarded tablet pressure).
const { app, engine, actions } = __h;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const { simulateNativePressure } = await import('/src/app/canvas/pressure.ts');
app.addDocument(await engine.newDocument({ name: 'pressure', width: 1200, height: 600, ppi: 72, background: { kind: 'white' }, backgroundLayerName: 'bg' }));
await sleep(300);
app.tool = 'brush';
app.fg = '#000000';
Object.assign(app.paint.brush, { diameter: 60, hardness: 1, opacity: 1, flow: 1, spacing: 0.05, smoothing: 0, pressureSize: true, pressureOpacity: false, minSize: 0 });
app.zoomTo(1);
app.active.view = { ...app.active.view, cx: 600, cy: 300 };
await sleep(300);
const stage = document.querySelector('.stage');
const r = stage.getBoundingClientRect();
const vp = app.viewport;
const toView = (x, y) => ({ x: vp.w / 2 + (x - 600), y: vp.h / 2 + (y - 300) });
async function stroke(y, kind) {
  const ev = (type, x, p) => {
    const v = toView(x, y);
    if (kind === 'native') simulateNativePressure(p);
    return new PointerEvent(type, { bubbles: true, clientX: r.left + v.x, clientY: r.top + v.y, button: 0, buttons: 1, pointerId: 1, pointerType: kind === 'pen' ? 'pen' : 'mouse', pressure: kind === 'pen' ? p : 0.5 });
  };
  stage.dispatchEvent(ev('pointerdown', 100, 0.1));
  for (let k = 1; k <= 50; k++) {
    stage.dispatchEvent(ev('pointermove', 100 + k * 20, 0.1 + 0.9 * (k / 50)));
    await sleep(10);
  }
  stage.dispatchEvent(ev('pointerup', 1100, 1));
  await sleep(600);
}
// stroke height measured by sampling a vertical line of pixels
async function width(x, y) {
  let n = 0;
  for (let dy = -40; dy <= 40; dy++) {
    const c = await engine.sampleColor(app.active.info.id, x, y + dy, 1, true);
    if (c && c[0] < 128) n++;
  }
  return n;
}
await stroke(150, 'pen');
await stroke(420, 'native');
const out = { pen: { start: await width(150, 150), end: await width(1050, 150) }, native: { start: await width(150, 420), end: await width(1050, 420) } };
out.pass = out.pen.end > out.pen.start * 3 && out.native.end > out.native.start * 3;
return out;
