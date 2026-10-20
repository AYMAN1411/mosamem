#!/bin/zsh
# Crash-recovery test: edit → wait for autosave → kill -9 → relaunch → restore → compare pixels.
set -e
cd "${0:A:h}/.."
source ~/.cargo/env
H() { node tools/harness.mjs "$@"; }
launch() {
  : > /tmp/mosamem-crash.log
  (MOSAMEM_HARNESS=1 ./target/debug/mosamem >> /tmp/mosamem-crash.log 2>&1 &)
  for i in {1..60}; do grep -q "harness\] listening" /tmp/mosamem-crash.log && break; sleep 1; done
  for i in {1..60}; do H 'return !!window.__h' >/dev/null 2>&1 && break; sleep 1; done
}
pkill -f "target/debug/mosamem" || true
sleep 2
# the debug binary loads its UI from the Vite dev server
if ! curl -s -o /dev/null --max-time 2 http://localhost:1420/; then
  (npx vite --port 1420 > /tmp/mosamem-vite.log 2>&1 &)
  for i in {1..30}; do curl -s -o /dev/null http://localhost:1420/ && break; sleep 1; done
fi
cargo build -q -p mosamem
sleep 1
launch
H '
const {app, actions, engine}=__h; const sleep=ms=>new Promise(r=>setTimeout(r,ms));
await actions.openPaths(["/System/Library/Desktop Pictures/.wallpapers/Sonoma Horizon/Sonoma Horizon.heic"]);
await actions.layer.new();
await actions.layer.set(app.active.info.layers[1].id, {blend: "multiply", opacity: 0.6});
await actions.layer.duplicate();
await actions.layer.set(app.active.info.active, {blend: "screen", opacity: 0.4});
return "edited"'
BEFORE=$(H 'return await __h.engine.debugDocHash(__h.app.active.info.id)')
echo "before: $BEFORE"
sleep 4   # autosave fires ~2 s after the last change
ls -la ~/Library/Application\ Support/app.mosamem.editor/recovery/ | grep -c mosamem | xargs echo "recovery files:"
PID=$(pgrep -f "target/debug/mosamem" | head -1)
kill -9 $PID
echo "killed $PID"
sleep 1
launch
sleep 2
tools/front.sh on; tools/shot.sh shots/m2-recovery.png; tools/front.sh off
LIST=$(H 'return await __h.engine.recoveryList()')
echo "$LIST" | head -12
AFTER=$(H '
const files = await __h.engine.recoveryList();
const info = await __h.engine.recoveryOpen(files[0].file);
__h.app.addDocument(info);
return await __h.engine.debugDocHash(info.id)')
echo "after:  $AFTER"
if [[ "$BEFORE" == "$AFTER" ]]; then echo "PASS: recovered pixels are identical"; else echo "FAIL"; exit 1; fi
