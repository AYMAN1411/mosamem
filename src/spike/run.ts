// Milestone 0 spike runner. Launch with VITE_SPIKE=1; results are written to spikes/out/*.json.
import { invoke } from '@tauri-apps/api/core';
import { GpuSpike, TILE } from './gpu';

const DOC_W = 6000;
const DOC_H = 4000;
const LAYERS = Number(import.meta.env.VITE_SPIKE_LAYERS ?? 30);

const log = (msg: string) => {
  const el = document.getElementById('app')!;
  el.textContent += msg + '\n';
  console.log(msg);
};

const report = (name: string, data: unknown) => invoke('spike_report', { name, json: JSON.stringify(data, null, 2) });
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
// Hidden/occluded windows throttle requestAnimationFrame, so frames are driven by a timer and each
// frame's cost is measured synchronously (submit + gl.finish).
const nextFrame = () => new Promise<number>((r) => setTimeout(() => r(performance.now()), 0));

function stats(xs: number[]) {
  const s = [...xs].sort((a, b) => a - b);
  const q = (p: number) => s[Math.min(s.length - 1, Math.floor(p * s.length))];
  return { n: s.length, p50: +q(0.5).toFixed(2), p95: +q(0.95).toFixed(2), p99: +q(0.99).toFixed(2), max: +s[s.length - 1].toFixed(2) };
}

async function environment() {
  const c = document.createElement('canvas');
  const gl = c.getContext('webgl2')!;
  const dbg = gl.getExtension('WEBGL_debug_renderer_info');
  return {
    userAgent: navigator.userAgent,
    crossOriginIsolated: self.crossOriginIsolated,
    sharedArrayBuffer: typeof SharedArrayBuffer !== 'undefined',
    webgpu: 'gpu' in navigator,
    devicePixelRatio: devicePixelRatio,
    screen: [screen.width, screen.height],
    renderer: dbg ? gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER),
    maxTextureSize: gl.getParameter(gl.MAX_TEXTURE_SIZE),
    maxArrayLayers: gl.getParameter(gl.MAX_ARRAY_TEXTURE_LAYERS),
    extensions: gl.getSupportedExtensions(),
    hardwareConcurrency: navigator.hardwareConcurrency,
    wasmMemory64: WebAssembly.validate(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 5, 3, 1, 4, 1])),
  };
}

async function ipc() {
  const out: Record<string, unknown> = {};
  // custom URI scheme: 256 KB tiles
  const base = navigator.userAgent.includes('Windows') ? 'http://tile.localhost/' : 'tile://localhost/';
  for (const conc of [1, 8]) {
    const n = 512;
    const t0 = performance.now();
    let next = 0;
    let bytes = 0;
    await Promise.all(
      Array.from({ length: conc }, async () => {
        while (next < n) {
          const i = next++;
          const buf = await (await fetch(base + i)).arrayBuffer();
          bytes += buf.byteLength;
        }
      }),
    );
    const ms = performance.now() - t0;
    out[`scheme_tiles_conc${conc}`] = { tiles: n, ms: +ms.toFixed(1), MBps: +(bytes / 1e6 / (ms / 1000)).toFixed(1), msPerTile: +(ms / n).toFixed(3) };
  }
  // invoke raw download / upload, 10 MB
  const MB10 = 10 * 1024 * 1024;
  const dl: number[] = [];
  const ul: number[] = [];
  const payload = new Uint8Array(MB10);
  for (let i = 0; i < 5; i++) {
    let t = performance.now();
    await invoke('spike_download', { len: MB10 });
    dl.push(performance.now() - t);
    t = performance.now();
    await invoke('spike_upload', payload);
    ul.push(performance.now() - t);
  }
  out.invoke_download_10MB_ms = stats(dl);
  out.invoke_upload_10MB_ms = stats(ul);
  return out;
}

async function gpu() {
  const canvas = document.createElement('canvas');
  canvas.style.cssText = 'position:fixed;inset:0;width:100vw;height:100vh';
  document.body.appendChild(canvas);
  const w = Math.round(innerWidth * devicePixelRatio);
  const h = Math.round(innerHeight * devicePixelRatio);
  canvas.width = w;
  canvas.height = h;
  const g = new GpuSpike({ canvas, docW: DOC_W, docH: DOC_H, layers: LAYERS });
  g.resize(w, h);
  const fit = Math.min(w / DOC_W, h / DOC_H) * 0.95;

  // refresh baseline
  const idle: number[] = [];
  let last = await nextFrame();
  for (let i = 0; i < 30; i++) {
    const t = await nextFrame();
    idle.push(t - last);
    last = t;
  }

  const results: Record<string, unknown> = { canvas: [w, h], atlasSlots: g.capacity, refreshIntervalMs: stats(idle) };
  type Path = (u: number) => { zoom: number; cx: number; cy: number };
  const scenarios: [string, number, Path][] = [
    ['zoom_fit_oscillate', 5000, (u) => ({ zoom: fit * (1 + 1.5 * (0.5 - 0.5 * Math.cos(u * Math.PI * 4))), cx: DOC_W / 2, cy: DOC_H / 2 })],
    ['pan_100pct', 6000, (u) => ({ zoom: 1, cx: w / 2 + (DOC_W - w) * u, cy: DOC_H / 2 + 600 * Math.sin(u * Math.PI * 2) })],
    ['pan_200pct', 6000, (u) => ({ zoom: 2, cx: 1500 + 3000 * u, cy: 2000 + 800 * Math.sin(u * Math.PI * 2) })],
    ['zoom_sweep_10_to_400pct', 6000, (u) => ({ zoom: 0.1 * Math.pow(40, u), cx: DOC_W / 2 + 200 * u, cy: DOC_H / 2 })],
  ];
  for (const [name, dur, path] of scenarios) {
    // warm-up: make the first view resident
    g.uploadBudget = 1e9;
    g.frame(path(0).zoom, path(0).cx, path(0).cy);
    g.gl.finish();
    g.uploadBudget = 96;
    const intervals: number[] = [];
    const cpu: number[] = [];
    let missingFrames = 0;
    let uploads = 0;
    const t0 = await nextFrame();
    last = t0;
    for (;;) {
      const t = await nextFrame();
      intervals.push(t - last);
      last = t;
      const u = (t - t0) / dur;
      if (u > 1) break;
      const p = path(u);
      const c0 = performance.now();
      const f = g.frame(p.zoom, p.cx, p.cy);
      g.gl.finish();
      cpu.push(performance.now() - c0);
      if (f.missing) missingFrames++;
      uploads += f.uploads;
    }
    results[name] = {
      visible: !document.hidden,
      frameCostMs: stats(cpu),
      framesOver16ms: cpu.filter((x) => x > 16.7).length,
      framesOver8ms: cpu.filter((x) => x > 8.3).length,
      framesWithMissingTiles: missingFrames,
      tileUploads: uploads,
    };
    log(`${name}: ${JSON.stringify(results[name])}`);
  }

  // GPU cost per full 30-layer composite, synchronous (gl.finish)
  const sync: number[] = [];
  for (let i = 0; i < 60; i++) {
    const t = performance.now();
    g.frame(1, DOC_W / 2, DOC_H / 2);
    g.gl.finish();
    sync.push(performance.now() - t);
  }
  results.syncComposite100pctMs = stats(sync);
  canvas.remove();
  return results;
}

async function memory() {
  // Holds every layer's full-resolution tiles in the webview, then hides the window to see
  // whether WebKit's inactive-page limit reloads the page.
  const tilesPerLayer = Math.ceil(DOC_W / TILE) * Math.ceil(DOC_H / TILE);
  const keep: Uint8Array[] = [];
  const t0 = performance.now();
  for (let l = 0; l < LAYERS; l++)
    for (let i = 0; i < tilesPerLayer; i++) {
      const b = new Uint8Array(TILE * TILE * 4);
      b.fill((l * 31 + i) & 255);
      keep.push(b);
    }
  const allocMs = performance.now() - t0;
  const bytes = keep.length * TILE * TILE * 4;
  sessionStorage.setItem('spike-stage', 'memory-hidden');
  await report('memory-progress', { allocatedGB: +(bytes / 1e9).toFixed(2), allocMs: Math.round(allocMs), stage: 'allocated, hiding window' });
  await sleep(30000);
  const r1 = { allocatedGB: +(bytes / 1e9).toFixed(2), allocMs: Math.round(allocMs), pageHidden: document.hidden, survived30s: true };
  await report('memory-progress', { ...r1, stage: 'growing to 5 GB' });
  sessionStorage.setItem('spike-stage', 'memory-5GB');
  while (keep.length * TILE * TILE * 4 < 5e9) {
    const b = new Uint8Array(TILE * TILE * 4);
    b.fill(keep.length & 255);
    keep.push(b);
  }
  await sleep(30000);
  sessionStorage.removeItem('spike-stage');
  return { first: r1, second: { allocatedGB: +((keep.length * TILE * TILE * 4) / 1e9).toFixed(2), pageHidden: document.hidden, survived30s: true } };
}

export async function runSpikes() {
  const app = document.getElementById('app')!;
  app.style.cssText = 'white-space:pre;font:12px monospace;color:#ddd;background:#1c1e24;position:fixed;inset:0;padding:8px;direction:ltr;z-index:1';
  const stage = sessionStorage.getItem('spike-stage');
  if (stage) {
    await report('memory', { survived: false, note: `page reloaded during stage ${stage}` });
    sessionStorage.removeItem('spike-stage');
    await invoke('spike_exit');
    return;
  }
  try {
    log('environment…');
    await report('environment', await environment());
    log('ipc…');
    await report('ipc', await ipc());
    log('gpu…');
    app.style.zIndex = '-1';
    await report(`gpu-${LAYERS}layers`, await gpu());
    app.style.zIndex = '1';
    if (!import.meta.env.VITE_SPIKE_GPU_ONLY) {
      log('memory…');
      await report('memory', await memory());
    }
  } catch (e) {
    await report('error', { error: String(e), stack: (e as Error).stack });
  }
  await invoke('spike_exit');
}
