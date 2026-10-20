#!/bin/zsh
# Scripted M1 screenshots (app must run with LAWHA_HARNESS=1).
cd "${0:A:h}/.."
H() { node tools/harness.mjs "$@"; }
tools/front.sh on
H '
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const {app, i18n, dock, actions}=__h;
i18n.set("ar"); dock.reset();
await actions.openPaths(["/System/Library/Desktop Pictures/.wallpapers/Sonoma Horizon/Sonoma Horizon.heic"]);
const d=app.active;
d.guides=[{id:901,axis:"h",pos:720},{id:902,axis:"v",pos:1280},{id:903,axis:"v",pos:2560}];
app.showGrid=true; app.gridSpacing=480; app.gridSubdivisions=4;
dock.flyout="history";
const c=window.__lawha.compositor; for(let i=0;i<300 && !c.settled();i++) await sleep(20);
await sleep(300); return d.info.width+"x"+d.info.height' && tools/shot.sh shots/m1-guides-grid-ar.png
H '
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const {app, i18n, dock}=__h;
dock.flyout=null; app.showGrid=false; i18n.set("en");
dock.apply("layers",{kind:"column",index:0});
dock.find("color").stack.collapsed=true;
app.tool="rotateView"; app.rotateTo(-12); app.zoomTo(0.5);
const c=window.__lawha.compositor; for(let i=0;i<300 && !c.settled();i++) await sleep(20);
await sleep(300); return "ok"' && tools/shot.sh shots/m1-dock-rotate-en.png
H '
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const {app, i18n, dock}=__h; dock.reset(); app.rotateTo(0); app.fit(); app.tool="move"; app.showGrid=false; app.dialog="new"; await sleep(400); return "ok"' && tools/shot.sh shots/m1-newdoc-en.png
H '__h.app.dialog=null; __h.i18n.set("ar"); return 1' >/dev/null
tools/front.sh off
