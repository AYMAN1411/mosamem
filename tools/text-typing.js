// The Text tool as a user meets it, run in the app (tools/harness.mjs -f, window in front):
// a quick click opens the editor WITH the keyboard focus, typing makes a text layer, a click on
// that text opens it again with the focus, and an addition updates the layer.
// (Before 2026-10-21 the editor appeared without focus, so typing went nowhere: it focused itself
// only once, when the canvas was built.)
const { app, engine } = __h;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
if (!document.hasFocus()) throw new Error('bring the window to the front first (tools/front.sh on)');
app.addDocument(await engine.newDocument({ name: 'text check', width: 800, height: 500, ppi: 72, background: { kind: 'white' }, backgroundLayerName: 'bg' }));
await sleep(500);
app.tool = 'text';
await sleep(200);
const canvas = [...document.querySelectorAll('canvas')].sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight)[0];
const rc = canvas.getBoundingClientRect();
const click = async (x, y, gap) => {
  const target = document.elementFromPoint(x, y);
  const ev = (type) => target.dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, bubbles: true, pointerId: 1, button: 0, buttons: type === 'pointerdown' ? 1 : 0, isPrimary: true }));
  ev('pointerdown');
  if (gap) await sleep(gap);
  ev('pointerup');
  await sleep(600);
};
const editor = () => document.querySelector('.editor[contenteditable]');
const cx = rc.left + rc.width / 2, cy = rc.top + rc.height / 2;
const checks = {};
await click(cx, cy, 0); // a quick click: press and release together
checks.quickClickOpensEditor = !!editor();
checks.editorHasFocus = document.activeElement === editor();
// keys the app might take for itself must reach the editor (Space pans the view elsewhere)
checks.keysReachEditor = ['Space', 'Enter', 'ArrowLeft', 'Backspace', 'KeyV', 'KeyT', 'BracketLeft', 'Digit5'].every((code) => {
  const key = { Space: ' ', Enter: 'Enter', ArrowLeft: 'ArrowLeft', Backspace: 'Backspace', KeyV: 'v', KeyT: 't', BracketLeft: '[', Digit5: '5' }[code];
  const ev = new KeyboardEvent('keydown', { key, code, bubbles: true, cancelable: true });
  editor().dispatchEvent(ev);
  return !ev.defaultPrevented;
});
document.execCommand('insertText', false, 'مرحبا Mosamem'); // goes to the focused element, like typing
await sleep(300);
await click(rc.left + 40, rc.top + 40, 100); // a click elsewhere commits
checks.typingMakesALayer = app.active.info.layers.some((l) => l.kind === 'text' && l.name === 'مرحبا Mosamem');
await click(cx - 30, cy - 15, 100); // a click on the text edits it
checks.reEditHasFocus = document.activeElement === editor() && editor()?.innerText === 'مرحبا Mosamem';
document.execCommand('insertText', false, ' ٢٠٢٦');
await sleep(300);
await click(rc.left + 40, rc.top + 40, 100);
checks.editUpdatesTheLayer = app.active.info.layers.some((l) => l.kind === 'text' && l.name === 'مرحبا Mosamem ٢٠٢٦');
checks.passed = Object.values(checks).every(Boolean);
return checks;
