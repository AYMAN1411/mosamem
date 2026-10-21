// Paths and selections as a user meets them, run in the app (tools/harness.mjs -f): the Pen in
// Path mode draws a work path with clicks, the options bar's buttons fill it, load it as a
// selection, turn an elliptical selection back into a path, and stroke that path; undo takes the
// stroke back.
const { app, engine } = __h;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
app.dialog = null; // after a page reload the recovery dialog may be up, covering the canvas
app.addDocument(await engine.newDocument({ name: 'paths check', width: 800, height: 500, ppi: 72, background: { kind: 'white' }, backgroundLayerName: 'bg' }));
await sleep(1000);
const id = app.active.info.id;
const canvas = [...document.querySelectorAll('canvas')].sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight)[0];
const d = app.active;
const at = (x, y) => {
  const rc = canvas.getBoundingClientRect();
  return [rc.left + rc.width / 2 + (x - d.view.cx) * d.view.zoom, rc.top + rc.height / 2 + (y - d.view.cy) * d.view.zoom];
};
const send = (type, [x, y], buttons) =>
  document.elementFromPoint(x, y).dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, bubbles: true, pointerId: 1, button: 0, buttons, isPrimary: true }));
const click = async (p) => {
  send('pointerdown', at(...p), 1);
  send('pointerup', at(...p), 0);
  await sleep(250);
};
// the options bar's buttons, found by their label in the interface language
const button = (key) => [...document.querySelectorAll('.options button')].find((b) => b.textContent.trim() === __h.t(key));
const press = async (key) => {
  button(key).click();
  await sleep(600);
};
const color = (x, y) => engine.sampleColor(id, x, y, 1, true);
const checks = {};

app.fg = '#000000';
app.tool = 'pen';
await sleep(150);
const mode = document.querySelector('.options select.mode');
mode.value = 'path';
mode.dispatchEvent(new Event('change', { bubbles: true }));
await sleep(150);
const layers = app.active.info.layers.length;
await click([100, 100]);
await click([300, 100]);
await click([200, 300]);
await click([100, 100]); // closes it
checks.workPathWithoutALayer = app.active.workPath?.subs[0].closed === true && app.active.workPath.subs[0].knots.length === 3 && app.active.info.layers.length === layers;
checks.buttonsShow = !!button('path.fill') && !!button('path.toSelection');

await press('path.fill');
checks.fillPaintsTheArea = (await color(200, 150))[0] === 0 && (await color(400, 150))[0] === 255;

await press('path.toSelection');
checks.pathBecomesASelection = app.active.info.hasSelection;

app.update(await engine.selectShape(id, { kind: 'ellipse', x0: 450, y0: 150, x1: 750, y1: 450, antialias: true }, 'replace', 0));
await sleep(200);
await press('path.fromSelection');
const sub = app.active.workPath?.subs[0];
checks.selectionBecomesASmoothPath = app.active.workPath?.subs.length === 1 && sub.knots.length >= 8 && sub.knots.length < 60 && sub.knots.some((k) => k.smooth);

app.update(await engine.selectSimple(id, 'deselect'));
app.fg = '#ff0000';
app.paint.brush.diameter = 8;
await sleep(150);
app.tool = 'pen';
await sleep(150);
await press('path.stroke');
const edge = await color(600, 152);
checks.strokeFollowsTheEdge = edge[0] > 200 && edge[1] < 60;
checks.strokeLeavesTheMiddle = (await color(600, 300))[1] === 255;
app.update(await engine.undo(id));
await sleep(300);
checks.undoTakesTheStrokeBack = (await color(600, 152))[1] === 255;

mode.value = 'shape';
mode.dispatchEvent(new Event('change', { bubbles: true }));
checks.passed = Object.values(checks).every(Boolean);
return checks;
