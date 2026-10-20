// The shape tools as a user meets them, run in the app (tools/harness.mjs -f): drags on the canvas
// with pointer events draw a rectangle (Shift: a square), an ellipse from its center (Alt), a
// polygon, a star and an arrow; the options bar changes the selected shape; the Properties panel
// edits its size; undo takes the edit back.
const { app, engine } = __h;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
app.dialog = null; // after a page reload the recovery dialog may be up, covering the canvas
app.addDocument(await engine.newDocument({ name: 'shapes check', width: 800, height: 500, ppi: 72, background: { kind: 'white' }, backgroundLayerName: 'bg' }));
await sleep(1000); // the window's panels settle after a reload, and the view refits
const canvas = [...document.querySelectorAll('canvas')].sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight)[0];
const d = app.active;
// document px → client px (the view is not turned), measured at each use
const at = (x, y) => {
  const rc = canvas.getBoundingClientRect();
  return [rc.left + rc.width / 2 + (x - d.view.cx) * d.view.zoom, rc.top + rc.height / 2 + (y - d.view.cy) * d.view.zoom];
};
const drag = async (from, to, mods = {}) => {
  const [x0, y0] = at(...from);
  const [x1, y1] = at(...to);
  const target = document.elementFromPoint(x0, y0);
  const ev = (type, x, y, buttons) => target.dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, bubbles: true, pointerId: 1, button: 0, buttons, isPrimary: true, ...mods }));
  ev('pointerdown', x0, y0, 1);
  for (let k = 1; k <= 8; k++) {
    ev('pointermove', x0 + ((x1 - x0) * k) / 8, y0 + ((y1 - y0) * k) / 8, 1);
    await sleep(16);
  }
  ev('pointerup', x1, y1, 0);
  await sleep(500);
};
const shapes = () => {
  const out = [];
  const walk = (ls) => ls.forEach((l) => (l.kind === 'shape' && out.push(l), l.children && walk(l.children)));
  walk(app.active.info.layers);
  return out;
};
const top = () => app.active.info.layers[0];
const near = (a, b) => Math.abs(a - b) < 0.6;
const checks = {};

app.tool = 'shapeRect';
await sleep(200);
checks.optionsBarShowsShapeFields = !!document.querySelector('.options input[type=color]');
await drag([100, 100], [260, 180]);
let s = top().shape?.shape;
checks.rectangle = top().kind === 'shape' && s.kind === 'rect' && near(s.x, 100) && near(s.y, 100) && near(s.w, 160) && near(s.h, 80);
checks.namedInTheUiLanguage = /\d$/.test(top().name);
await drag([300, 100], [400, 160], { shiftKey: true });
s = top().shape.shape;
checks.shiftMakesASquare = s.kind === 'rect' && near(s.w, s.h) && near(s.w, 100);

app.tool = 'shapeEllipse';
await sleep(100);
await drag([550, 150], [600, 180], { altKey: true });
s = top().shape.shape;
checks.altDrawsFromTheCenter = s.kind === 'ellipse' && near(s.x, 500) && near(s.y, 120) && near(s.w, 100) && near(s.h, 60);

app.tool = 'shapePolygon';
await sleep(100);
await drag([150, 350], [150, 290]);
s = top().shape.shape;
checks.polygon = s.kind === 'polygon' && near(s.r, 60) && s.sides === 6 && Math.abs(s.angle) < 0.5;

app.tool = 'shapeStar';
await sleep(100);
await drag([350, 350], [350, 280]);
s = top().shape.shape;
checks.star = s.kind === 'star' && near(s.r, 70) && s.points === 5;

app.tool = 'shapeLine';
await sleep(100);
await drag([500, 300], [700, 420]);
s = top().shape.shape;
checks.arrowLine = s.kind === 'line' && s.arrowEnd && !s.arrowStart && top().shape.paint.stroke?.width > 0 && top().shape.paint.fill === null;
checks.sixShapeLayers = shapes().length === 6;

// the options bar changes the selected shape: a stroke on the star
const star = shapes().find((l) => l.shape.shape.kind === 'star');
app.update(await engine.setActive(app.active.info.id, star.id));
app.tool = 'shapeStar';
await sleep(200);
const strokeBox = [...document.querySelectorAll('.options input[type=checkbox]')][1];
strokeBox.checked = true;
strokeBox.dispatchEvent(new Event('change', { bubbles: true }));
await sleep(500);
const star2 = shapes().find((l) => l.id === star.id);
checks.optionsBarAddsAStroke = !!star2.shape.paint.stroke && star2.shape.paint.stroke.width === 4;
const pointsField = document.querySelector('.options input[type=number]');
pointsField.value = '8';
pointsField.dispatchEvent(new Event('change', { bubbles: true }));
await sleep(500);
checks.optionsBarChangesPoints = shapes().find((l) => l.id === star.id).shape.shape.points === 8;

// the Properties panel edits the rectangle's width; undo takes it back
const rect = shapes().find((l) => l.shape.shape.kind === 'rect' && near(l.shape.shape.w, 160));
app.update(await engine.setActive(app.active.info.id, rect.id));
__h.dock.show('properties');
await sleep(400);
const wField = document.querySelectorAll('.shape .grid input')[2]; // x, y, w, h
checks.propertiesShowsGeometry = !!wField;
if (wField) {
  wField.value = '300';
  wField.dispatchEvent(new Event('change', { bubbles: true }));
  await sleep(500);
  checks.propertiesChangesWidth = near(shapes().find((l) => l.id === rect.id).shape.shape.w, 300);
  app.update(await engine.undo(app.active.info.id));
  await sleep(300);
  checks.undoRestoresIt = near(shapes().find((l) => l.id === rect.id).shape.shape.w, 160);
}
checks.passed = Object.values(checks).every(Boolean);
return checks;
