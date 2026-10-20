// The Pen and Direct Selection tools as a user meets them, run in the app (tools/harness.mjs -f):
// clicks and drags with pointer events draw a closed path with a curve and an open one, add and
// delete points on the outline, move points and handles, turn a rectangle into a free path,
// delete a point with the Delete key, and draw a work path in Path mode.
const { app, engine } = __h;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
app.dialog = null; // after a page reload the recovery dialog may be up, covering the canvas
app.addDocument(await engine.newDocument({ name: 'pen check', width: 800, height: 500, ppi: 72, background: { kind: 'white' }, backgroundLayerName: 'bg' }));
await sleep(1000);
const canvas = [...document.querySelectorAll('canvas')].sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight)[0];
const d = app.active;
const at = (x, y) => {
  const rc = canvas.getBoundingClientRect();
  return [rc.left + rc.width / 2 + (x - d.view.cx) * d.view.zoom, rc.top + rc.height / 2 + (y - d.view.cy) * d.view.zoom];
};
// sent to whatever is under the point, as a real pointer would be
const send = (type, [x, y], buttons, mods = {}) =>
  document.elementFromPoint(x, y).dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, bubbles: true, pointerId: 1, button: 0, buttons, isPrimary: true, ...mods }));
const drag = async (from, to, mods = {}) => {
  const [a, b] = [at(...from), at(...to)];
  send('pointerdown', a, 1, mods);
  for (let k = 1; k <= 6; k++) {
    send('pointermove', [a[0] + ((b[0] - a[0]) * k) / 6, a[1] + ((b[1] - a[1]) * k) / 6], 1, mods);
    await sleep(16);
  }
  send('pointerup', b, 0, mods);
  await sleep(400);
};
const click = (p, mods) => drag(p, p, mods);
const key = async (k) => {
  document.body.dispatchEvent(new KeyboardEvent('keydown', { key: k, code: k, bubbles: true, cancelable: true }));
  await sleep(400);
};
const top = () => app.active.info.layers[0];
const active = () => __h.bridge.findLayer(app.active.info.layers, app.active.info.active);
const knots = () => active().shape.shape.path.subs[0].knots;
const near = (p, x, y) => Math.abs(p[0] - x) < 0.6 && Math.abs(p[1] - y) < 0.6;
const checks = {};

app.tool = 'pen';
await sleep(200);
await click([100, 100]);
await click([300, 100]);
await drag([300, 300], [360, 300]); // a smooth point
await click([100, 300]);
await click([100, 100]); // on the first point: closes the path
let s = top().shape?.shape;
checks.closedPathBecomesAShapeLayer = s?.kind === 'free' && s.path.subs[0].closed && s.path.subs[0].knots.length === 4;
const k2 = s?.path.subs[0].knots[2];
checks.dragMakesASmoothPoint = !!k2 && k2.smooth && near(k2.o, 360, 300) && near(k2.i, 240, 300);

await click([500, 100]);
await click([700, 100]);
await key('Enter');
s = top().shape?.shape;
checks.enterFinishesAnOpenPath = s?.kind === 'free' && !s.path.subs[0].closed && s.path.subs[0].knots.length === 2;

// back to the closed path: a click on its outline adds a point, a click on a point deletes it
const closed = app.active.info.layers.find((l) => l.shape?.shape.kind === 'free' && l.shape.shape.path.subs[0].closed);
app.update(await engine.setActive(app.active.info.id, closed.id));
await sleep(200);
await click([200, 100]);
checks.clickOnOutlineAddsAPoint = knots().length === 5 && near(knots()[1].p, 200, 100);
await click([200, 100]);
checks.clickOnPointDeletesIt = knots().length === 4;

app.tool = 'directSelect';
await sleep(200);
await drag([100, 300], [80, 350]);
checks.dragMovesAPoint = near(knots()[3].p, 80, 350);
app.update(await engine.undo(app.active.info.id));
await sleep(300);
checks.undoPutsItBack = near(knots()[3].p, 100, 300);

await click([300, 300]); // select the smooth point, then pull its handle
await drag([360, 300], [300, 360]);
const sm = knots()[2];
checks.handleTurnsAndTheOtherFollows = near(sm.o, 300, 360) && near(sm.i, 300, 240);

await click([100, 300]);
await key('Delete');
checks.deleteRemovesTheSelectedPoint = knots().length === 3;

// a rectangle becomes a free path once a corner moves
app.tool = 'shapeRect';
await sleep(150);
await drag([450, 250], [650, 400]);
app.tool = 'directSelect';
await sleep(150);
await drag([650, 400], [700, 450]);
s = active().shape.shape; // a new layer goes above the active one
checks.rectangleBecomesAFreePath = s.kind === 'free' && s.path.subs[0].knots.some((k) => near(k.p, 700, 450));

// Path mode: a work path, no new layer
app.tool = 'pen';
await sleep(150);
document.querySelector('.options select.mode').value = 'path';
document.querySelector('.options select.mode').dispatchEvent(new Event('change', { bubbles: true }));
await sleep(150);
const layersBefore = app.active.info.layers.length;
app.update(await engine.setActive(app.active.info.id, app.active.info.layers[app.active.info.layers.length - 1].id)); // the background
await click([50, 450]);
await click([150, 420]);
await click([250, 460]);
await key('Escape');
checks.pathModeMakesAWorkPath = app.active.workPath?.subs[0].knots.length === 3 && app.active.info.layers.length === layersBefore;
document.querySelector('.options select.mode').value = 'shape';
document.querySelector('.options select.mode').dispatchEvent(new Event('change', { bubbles: true }));

checks.passed = Object.values(checks).every(Boolean);
return checks;
