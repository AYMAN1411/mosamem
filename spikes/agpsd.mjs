// M0 spike: ag-psd write + read of a 6000×4000, 30-layer PSD (raw imageData, no canvas).
import { writePsdUint8Array, readPsd, initializeCanvas } from 'ag-psd';
initializeCanvas(() => { throw new Error('no canvas'); }, (w, h) => ({ width: w, height: h, data: new Uint8ClampedArray(w * h * 4) }));
import { writeFileSync, readFileSync, mkdirSync, statSync } from 'node:fs';

const W = 6000, H = 4000, L = 30;
const mb = () => Math.round(process.memoryUsage().rss / 1e6);
const layer = (i) => {
  const w = i === 0 ? W : 2400, h = i === 0 ? H : 1800;
  const data = new Uint8ClampedArray(w * h * 4);
  const u = new Uint32Array(data.buffer);
  for (let y = 0; y < h; y++) u.subarray(y * w, y * w + w).fill(((i === 0 ? 255 : 200) << 24) | ((y * 7 + i * 40) & 255) << 8 | (i * 23 & 255));
  const left = i === 0 ? 0 : (i * 137) % (W - w), top = i === 0 ? 0 : (i * 89) % (H - h);
  return { name: `طبقة ${i}`, left, top, imageData: { width: w, height: h, data }, blendMode: ['normal', 'multiply', 'screen', 'overlay'][i % 4], opacity: 0.9 };
};
let t = performance.now();
const psd = { width: W, height: H, children: Array.from({ length: L }, (_, i) => layer(i)),
  imageData: { width: W, height: H, data: new Uint8ClampedArray(W * H * 4).fill(128) } };
const gen = performance.now() - t;
t = performance.now();
const bytes = writePsdUint8Array(psd, { compress: true });
const write = performance.now() - t;
mkdirSync('spikes/out', { recursive: true });
writeFileSync('spikes/out/big.psd', bytes);
const rssAfterWrite = mb();
t = performance.now();
const back = readPsd(readFileSync('spikes/out/big.psd'), { useImageData: true });
const read = performance.now() - t;
const res = { size: `${W}x${H}`, layers: back.children.length, firstName: back.children[1].name, fileMB: +(statSync('spikes/out/big.psd').size / 1e6).toFixed(1),
  generateMs: Math.round(gen), writeMs: Math.round(write), readMs: Math.round(read), rssAfterWriteMB: rssAfterWrite, rssAfterReadMB: mb(),
  note: 'layers 1..29 are 2400x1800 tiles; background is full size; compress (ZIP) on write' };
writeFileSync('spikes/out/agpsd.json', JSON.stringify(res, null, 2));
console.log(res);
