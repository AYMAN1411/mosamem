#!/bin/zsh
# M10: shows that the AI features make no network requests once their models are installed.
#
# 1. Engine, sandboxed: the AI integration test (install from a local folder, then Select Subject,
#    Object Selection and Smart Fill on photos) runs inside a macOS sandbox profile that denies
#    every outbound connection except to this machine. A control test in the same sandbox tries a
#    real download, which must fail.
# 2. App, watched: the debug app runs every AI feature through the test harness while its sockets
#    are sampled every 50 ms; any connection to another host is recorded. (The app can't run inside
#    that sandbox: its web view stops loading, since WebKit's own processes need more than it allows.)
#
#   tools/ai/no-network.sh <model files dir> <photos dir> <photo.png> <report.json>
# Needs: the models installed in the app, port 1420 free, no other Mosamem running.
set -e
cd "${0:A:h}/../.."
models="${1:A}"; photos="${2:A}"; photo="${3:A}"; report="${4:A}"
profile=$(mktemp -t mosamem-nonet).sb
cat > "$profile" <<'SB'
(version 1)
(allow default)
(deny network-outbound)
(allow network-outbound (remote ip "localhost:*"))
(allow network-outbound (remote unix-socket))
SB

# ---- 1. the engine in the sandbox ----
bin=$(cargo test -p mosamem --lib --no-run --message-format=json 2>/dev/null | node -e '
let last = ""; require("readline").createInterface({ input: process.stdin }).on("line", (l) => {
  try { const m = JSON.parse(l); if (m.profile && m.profile.test && m.executable && m.target.name === "mosamem_lib") last = m.executable; } catch {}
}).on("close", () => console.log(last));')
export MOSAMEM_MODEL_DIR="$models" MOSAMEM_TEST_PHOTOS="$photos"
engine_ok=false; control_ok=false
sandbox-exec -f "$profile" "$bin" ai::tests::models_on_photos --ignored --exact > /tmp/mosamem-nonet-engine.log 2>&1 && engine_ok=true
sandbox-exec -f "$profile" "$bin" ai::tests::upstream_download_is_blocked --ignored --exact > /tmp/mosamem-nonet-control.log 2>&1 && control_ok=true

# ---- 2. the app, watched ----
npx vite --port 1420 --strictPort > /tmp/mosamem-nonet-vite.log 2>&1 &
vite=$!
MOSAMEM_HARNESS=1 ./target/debug/mosamem > /tmp/mosamem-nonet-app.log 2>&1 &
app=$!
trap 'kill $app $vite 2>/dev/null; rm -f "$profile"' EXIT
sleep 3; tools/front.sh on   # a covered window is throttled by WebKit
until curl -s -m 3 -X POST -d 'return typeof __h' http://127.0.0.1:47811/ 2>/dev/null | grep -q object; do sleep 1; done
sockets=$(mktemp)
( n=0; while kill -0 $app 2>/dev/null; do lsof -nP -a -p $app -i 2>/dev/null | tail -n +2; echo "#sample"; sleep 0.05; done ) > "$sockets" &
sampler=$!
script=$(mktemp)
cat > "$script" <<JS
const { app, engine, actions } = __h;
const out = { steps: [] };
const step = async (name, f) => { try { const r = await f(); if (r && r.id) app.update(r); out.steps.push([name, 'ok']); } catch (e) { out.steps.push([name, 'error: ' + (e.message ?? e)]); } };
await actions.openPaths(['$photo']);
await new Promise((r) => setTimeout(r, 500));
const id = app.active.info.id, w = app.active.info.width, h = app.active.info.height;
await step('select subject', () => engine.aiSelectSubject(id, 'replace'));
await step('object selection: click', () => engine.aiObjectSelect(id, { clicks: [{ x: w / 2, y: h / 2, add: true }], bbox: null, mode: 'replace', allLayers: false }));
await step('object selection: box', () => engine.aiObjectSelect(id, { clicks: [], bbox: [w * 0.2, h * 0.2, w * 0.8, h * 0.8], mode: 'replace', allLayers: false }));
await step('select a box', () => engine.selectShape(id, { kind: 'rect', x0: Math.round(w * 0.05), y0: Math.round(h * 0.05), x1: Math.round(w * 0.2), y1: Math.round(h * 0.2) }, 'replace', 0));
await step('smart fill (AI)', () => engine.aiSmartFill(id, { method: 'ai', newLayer: 'fill' }));
await step('smart fill (classic)', () => engine.aiSmartFill(id, { method: 'classic', newLayer: 'fill 2' }));
const photoLayer = app.active.info.layers[app.active.info.layers.length - 1].id;
await step('remove background', () => engine.aiRemoveBackground(id, photoLayer));
out.devices = (await engine.aiStatus()).map((p) => [p.id, p.gpu ? 'webgpu' : 'cpu']);
return out;
JS
node tools/harness.mjs -f "$script" > /tmp/mosamem-nonet-result.json
tools/front.sh off
sleep 0.5
kill $sampler 2>/dev/null || true
node -e '
const fs = require("fs");
const r = JSON.parse(fs.readFileSync("/tmp/mosamem-nonet-result.json", "utf8"));
const lines = fs.readFileSync(process.argv[1], "utf8").split("\n");
const samples = lines.filter((l) => l === "#sample").length;
const sockets = lines.filter((l) => l && l !== "#sample");
const outside = [...new Set(sockets.filter((l) => /TCP|UDP/.test(l) && !/127\.0\.0\.1|\[::1\]|localhost|\*:/.test(l)))];
const report = {
  engineInSandbox: { passed: process.argv[2] === "true", log: "/tmp/mosamem-nonet-engine.log" },
  controlDownloadBlocked: process.argv[3] === "true",
  app: { ...r, socketSamples: samples, localSocketLines: sockets.length, connectionsToOtherHosts: outside },
};
report.passed = report.engineInSandbox.passed && report.controlDownloadBlocked && r.steps.every(([, s]) => s === "ok") && outside.length === 0;
fs.writeFileSync(process.argv[4], JSON.stringify(report, null, 2) + "\n");
console.log(JSON.stringify(report, null, 2));
' "$sockets" "$engine_ok" "$control_ok" "$report"
rm -f "$sockets" "$script"
