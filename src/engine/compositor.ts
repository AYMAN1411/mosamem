// Display compositor: draws the active document's flattened tiles (streamed from the native engine)
// with WebGL2. Tiles live in a texture-array atlas addressed through per-mip page tables. While a
// tile is loading, the shader falls back to coarser mip levels, so the view never shows holes.

import { tileUrl } from './bridge';

export const TILE = 256;
const MAX_LOD = 6;
const CAPACITY = 1024; // atlas slots (256×256 RGBA8 each, 256 MB)
const EMPTY = 0xffff; // page entry: tile known to be fully transparent
const CONCURRENCY = 8;

export interface ViewTransform {
  /** Device pixels per document pixel. */
  zoom: number;
  /** Document point shown at the canvas center. */
  cx: number;
  cy: number;
  /** Radians, clockwise. */
  rotation: number;
}

export interface DisplayDoc {
  id: number;
  width: number;
  height: number;
  version: number;
}

const VS = `#version 300 es
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

const FS = `#version 300 es
precision highp float;
precision highp int;
precision highp usampler2DArray;
precision highp sampler2DArray;
uniform sampler2DArray uAtlas;
uniform usampler2DArray uPages;
uniform mat3 uScreenToDoc;
uniform vec2 uDoc;
uniform float uZoom;
uniform int uLod;
uniform int uMaxLod;
uniform vec3 uPasteboard;
uniform bool uPixelGrid;
uniform float uCanvasH;
out vec4 outColor;

// Premultiplied texel at integer lod coordinates. ok=false when that tile is not loaded yet.
vec4 texel(ivec2 p, int lod, out bool ok) {
  ivec2 t = p >> 8;
  uint e = texelFetch(uPages, ivec3(t, lod), 0).r;
  if (e == 0u) { ok = false; return vec4(0.0); }
  ok = true;
  if (e == ${EMPTY}u) return vec4(0.0);
  vec4 c = texelFetch(uAtlas, ivec3(p & 255, int(e) - 1), 0);
  return vec4(c.rgb * c.a, c.a);
}

vec4 sampleLod(vec2 doc, int lod, out bool ok) {
  float s = exp2(float(lod));
  ivec2 size = ivec2(ceil(uDoc / s));
  if (lod == 0 && uZoom >= 1.0) {
    return texel(clamp(ivec2(floor(doc)), ivec2(0), size - 1), 0, ok);
  }
  vec2 p = doc / s - 0.5;
  ivec2 i0 = ivec2(floor(p));
  vec2 f = p - vec2(i0);
  ivec2 lo = ivec2(0), hi = size - 1;
  bool a, b, c, d;
  vec4 t00 = texel(clamp(i0, lo, hi), lod, a);
  vec4 t10 = texel(clamp(i0 + ivec2(1, 0), lo, hi), lod, b);
  vec4 t01 = texel(clamp(i0 + ivec2(0, 1), lo, hi), lod, c);
  vec4 t11 = texel(clamp(i0 + ivec2(1, 1), lo, hi), lod, d);
  ok = a && b && c && d;
  return mix(mix(t00, t10, f.x), mix(t01, t11, f.x), f.y);
}

void main() {
  vec2 screen = vec2(gl_FragCoord.x, uCanvasH - gl_FragCoord.y);
  vec2 doc = (uScreenToDoc * vec3(screen, 1.0)).xy;
  if (doc.x < 0.0 || doc.y < 0.0 || doc.x >= uDoc.x || doc.y >= uDoc.y) {
    outColor = vec4(uPasteboard, 1.0);
    return;
  }
  vec4 c = vec4(0.0);
  bool ok = false;
  for (int l = uLod; l <= uMaxLod; l++) {
    c = sampleLod(doc, l, ok);
    if (ok) break;
  }
  // transparency checkerboard, fixed in screen space like most editors
  ivec2 q = ivec2(screen) / 8;
  float k = ((q.x + q.y) & 1) == 0 ? 0.80 : 0.66;
  vec3 rgb = c.rgb + vec3(k) * (1.0 - c.a);
  if (uPixelGrid && uZoom >= 8.0) {
    vec2 g = fract(doc) * uZoom;
    if (g.x < 1.0 || g.y < 1.0) rgb = mix(rgb, vec3(0.5), 0.35);
  }
  outColor = vec4(rgb, 1.0);
}`;

interface Entry {
  slot: number; // 1-based atlas slot, or EMPTY
  version: number;
}

export class Compositor {
  private gl: WebGL2RenderingContext;
  private prog: WebGLProgram;
  private atlas: WebGLTexture;
  private pages: WebGLTexture;
  private u: Record<string, WebGLUniformLocation | null> = {};
  private doc: DisplayDoc | null = null;
  private view: ViewTransform = { zoom: 1, cx: 0, cy: 0, rotation: 0 };
  private w = 1;
  private h = 1;
  private pageW = 1;
  private pageH = 1;
  private pageData: Uint16Array[] = [];
  private pagesDirty = new Set<number>();
  private entries = new Map<string, Entry>();
  private lru = new Map<number, string>(); // slot -> key, insertion order = least recently used first
  private free: number[] = [];
  private inflight = new Set<string>();
  private queue: { key: string; lod: number; tx: number; ty: number; prio: number }[] = [];
  private queued = new Set<string>();
  private active = 0;
  private frame = 0;
  private generation = 0;
  private drawnVersion = -1;
  pixelGrid = true;
  pasteboard: [number, number, number] = [0.086, 0.09, 0.106];
  /** Called after each drawn frame (for overlays, perf). */
  onFrame: ((ms: number) => void) | null = null;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext('webgl2', { antialias: false, alpha: false, depth: false, stencil: false, desynchronized: true, powerPreference: 'high-performance' });
    if (!gl) throw new Error('WebGL2 is not available');
    this.gl = gl;
    this.prog = program(gl, VS, FS);
    for (const n of ['uAtlas', 'uPages', 'uScreenToDoc', 'uDoc', 'uZoom', 'uLod', 'uMaxLod', 'uPasteboard', 'uPixelGrid', 'uCanvasH']) this.u[n] = gl.getUniformLocation(this.prog, n);
    this.atlas = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlas);
    gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.RGBA8, TILE, TILE, CAPACITY);
    nearest(gl, gl.TEXTURE_2D_ARRAY);
    this.pages = gl.createTexture()!;
    gl.bindVertexArray(gl.createVertexArray());
    this.resetAtlas();
  }

  private resetAtlas() {
    this.entries.clear();
    this.lru.clear();
    this.inflight.clear();
    this.queue = [];
    this.queued.clear();
    this.free = [];
    for (let i = CAPACITY; i >= 1; i--) this.free.push(i);
    this.generation++;
  }

  setDocument(doc: DisplayDoc | null) {
    const gl = this.gl;
    const same = this.doc && doc && this.doc.id === doc.id && this.doc.width === doc.width && this.doc.height === doc.height;
    if (same) {
      if (doc!.version !== this.doc!.version) {
        this.doc = { ...doc! };
        this.requestFrame(); // stale tiles stay visible until their replacements arrive
      }
      return;
    }
    this.doc = doc ? { ...doc } : null;
    this.resetAtlas();
    if (!doc) return this.requestFrame();
    this.pageW = Math.ceil(doc.width / TILE);
    this.pageH = Math.ceil(doc.height / TILE);
    this.pageData = Array.from({ length: MAX_LOD + 1 }, () => new Uint16Array(this.pageW * this.pageH));
    gl.deleteTexture(this.pages);
    this.pages = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.pages);
    gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.R16UI, this.pageW, this.pageH, MAX_LOD + 1);
    nearest(gl, gl.TEXTURE_2D_ARRAY);
    for (let l = 0; l <= MAX_LOD; l++) this.pagesDirty.add(l);
    this.requestFrame();
  }

  setView(view: ViewTransform) {
    this.view = { ...view };
    this.requestFrame();
  }

  resize(cssW: number, cssH: number, dpr: number) {
    const w = Math.max(1, Math.round(cssW * dpr));
    const h = Math.max(1, Math.round(cssH * dpr));
    if (w === this.w && h === this.h) return;
    this.canvas.width = this.w = w;
    this.canvas.height = this.h = h;
    this.requestFrame();
  }

  /** Coarsest lod that still has at least one pixel per tile row; used as the always-loaded base. */
  private baseLod(): number {
    if (!this.doc) return MAX_LOD;
    let l = 0;
    while (l < MAX_LOD && (Math.ceil(this.doc.width / (TILE << l)) > 2 || Math.ceil(this.doc.height / (TILE << l)) > 2)) l++;
    return l;
  }

  lodFor(zoom: number): number {
    return Math.max(0, Math.min(MAX_LOD, Math.floor(Math.log2(1 / zoom) + 1e-6)));
  }

  /** Screen (device px, y down) → document matrix as [a, b, c, d, e, f]: doc = (a·x + c·y + e, b·x + d·y + f). */
  screenToDoc(): [number, number, number, number, number, number] {
    const { zoom, cx, cy, rotation } = this.view;
    const cos = Math.cos(-rotation) / zoom;
    const sin = Math.sin(-rotation) / zoom;
    const ox = this.w / 2;
    const oy = this.h / 2;
    // doc = R(-θ)·(s − o)/zoom + c
    return [cos, sin, -sin, cos, cx - (cos * ox - sin * oy), cy - (sin * ox + cos * oy)];
  }

  requestFrame() {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.draw();
    });
  }

  private visibleTiles(lod: number): { tx0: number; ty0: number; tx1: number; ty1: number } {
    const m = this.screenToDoc();
    const corners = [
      [0, 0],
      [this.w, 0],
      [0, this.h],
      [this.w, this.h],
    ].map(([x, y]) => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]);
    const span = TILE << lod;
    const xs = corners.map((c) => c[0]);
    const ys = corners.map((c) => c[1]);
    const maxX = Math.ceil(this.doc!.width / span) - 1;
    const maxY = Math.ceil(this.doc!.height / span) - 1;
    return {
      tx0: Math.max(0, Math.floor(Math.min(...xs) / span)),
      ty0: Math.max(0, Math.floor(Math.min(...ys) / span)),
      tx1: Math.min(maxX, Math.floor(Math.max(...xs) / span)),
      ty1: Math.min(maxY, Math.floor(Math.max(...ys) / span)),
    };
  }

  private want(lod: number, tx: number, ty: number, prio: number) {
    const key = `${lod}/${tx}/${ty}`;
    const e = this.entries.get(key);
    if (e && e.slot !== EMPTY) {
      // mark as recently used
      this.lru.delete(e.slot);
      this.lru.set(e.slot, key);
    }
    if (e && e.version === this.doc!.version) return;
    if (this.inflight.has(key) || this.queued.has(key)) return;
    this.queued.add(key);
    this.queue.push({ key, lod, tx, ty, prio });
  }

  private pump() {
    this.queue.sort((a, b) => a.prio - b.prio);
    while (this.active < CONCURRENCY && this.queue.length) {
      const job = this.queue.shift()!;
      if (this.inflight.has(job.key)) continue;
      this.inflight.add(job.key);
      this.active++;
      const doc = this.doc!;
      const gen = this.generation;
      fetch(tileUrl(doc.id, doc.version, job.lod, job.tx, job.ty))
        .then(async (r) => (r.status === 200 ? new Uint8Array(await r.arrayBuffer()) : r.status === 204 ? null : Promise.reject(r.status)))
        .then((data) => {
          if (gen === this.generation) this.store(job.key, job.lod, job.tx, job.ty, doc.version, data);
        })
        .catch(() => {})
        .finally(() => {
          if (gen === this.generation) this.inflight.delete(job.key);
          this.active--;
          this.requestFrame();
        });
    }
  }

  private store(key: string, lod: number, tx: number, ty: number, version: number, data: Uint8Array | null) {
    const gl = this.gl;
    const prev = this.entries.get(key);
    if (prev && prev.version > version) return;
    let slot: number;
    if (data === null) {
      if (prev && prev.slot !== EMPTY) this.release(prev.slot);
      slot = EMPTY;
    } else {
      if (prev && prev.slot !== EMPTY) slot = prev.slot;
      else slot = this.allocate();
      gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlas);
      gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, 0, 0, slot - 1, TILE, TILE, 1, gl.RGBA, gl.UNSIGNED_BYTE, data);
      this.lru.delete(slot);
      this.lru.set(slot, key);
    }
    this.entries.set(key, { slot, version });
    this.pageData[lod][ty * this.pageW + tx] = slot;
    this.pagesDirty.add(lod);
  }

  private allocate(): number {
    const s = this.free.pop();
    if (s !== undefined) return s;
    const [slot, key] = this.lru.entries().next().value as [number, string];
    this.lru.delete(slot);
    const [lod, tx, ty] = key.split('/').map(Number);
    this.entries.delete(key);
    this.pageData[lod][ty * this.pageW + tx] = 0;
    this.pagesDirty.add(lod);
    return slot;
  }

  private release(slot: number) {
    this.lru.delete(slot);
    this.free.push(slot);
  }

  private draw() {
    const t0 = performance.now();
    const gl = this.gl;
    gl.viewport(0, 0, this.w, this.h);
    if (!this.doc) {
      gl.clearColor(...this.pasteboard, 1);
      gl.clear(gl.COLOR_BUFFER_BIT);
      return;
    }
    const lod = this.lodFor(this.view.zoom);
    const base = Math.max(lod, this.baseLod());
    // Requests are rebuilt every frame: the coarse base level first (the whole document in a few
    // tiles), then the visible tiles nearest the view center.
    this.queue = [];
    this.queued.clear();
    const baseSpan = TILE << base;
    for (let ty = 0; ty * baseSpan < this.doc.height; ty++) for (let tx = 0; tx * baseSpan < this.doc.width; tx++) this.want(base, tx, ty, -1e9);
    if (lod < base) {
      const v = this.visibleTiles(lod);
      const span = TILE << lod;
      for (let ty = v.ty0; ty <= v.ty1; ty++)
        for (let tx = v.tx0; tx <= v.tx1; tx++) {
          const d = Math.hypot((tx + 0.5) * span - this.view.cx, (ty + 0.5) * span - this.view.cy);
          this.want(lod, tx, ty, d);
        }
    }
    this.pump();

    if (this.pagesDirty.size) {
      gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.pages);
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 2);
      for (const l of this.pagesDirty) gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, 0, 0, l, this.pageW, this.pageH, 1, gl.RED_INTEGER, gl.UNSIGNED_SHORT, this.pageData[l]);
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 4);
      this.pagesDirty.clear();
    }

    const m = this.screenToDoc();
    gl.useProgram(this.prog);
    gl.uniformMatrix3fv(this.u.uScreenToDoc, false, new Float32Array([m[0], m[1], 0, m[2], m[3], 0, m[4], m[5], 1]));
    gl.uniform2f(this.u.uDoc, this.doc.width, this.doc.height);
    gl.uniform1f(this.u.uZoom, this.view.zoom);
    gl.uniform1i(this.u.uLod, lod);
    gl.uniform1i(this.u.uMaxLod, MAX_LOD);
    gl.uniform3f(this.u.uPasteboard, ...this.pasteboard);
    gl.uniform1i(this.u.uPixelGrid, this.pixelGrid ? 1 : 0);
    gl.uniform1f(this.u.uCanvasH, this.h);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlas);
    gl.uniform1i(this.u.uAtlas, 0);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.pages);
    gl.uniform1i(this.u.uPages, 1);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    this.drawnVersion = this.doc.version;
    this.onFrame?.(performance.now() - t0);
  }

  /** True when every tile needed for the current view at full detail is loaded and drawn. */
  settled(): boolean {
    return this.frame === 0 && this.queue.length === 0 && this.inflight.size === 0 && this.drawnVersion === this.doc?.version;
  }

  /** Tiles still wanted for the current view (queued or loading). */
  pending(): number {
    return this.queue.length + this.inflight.size;
  }

  destroy() {
    cancelAnimationFrame(this.frame);
    this.generation++;
    const lose = this.gl.getExtension('WEBGL_lose_context');
    lose?.loseContext();
  }
}

function nearest(gl: WebGL2RenderingContext, target: number) {
  gl.texParameteri(target, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(target, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texParameteri(target, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(target, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
}

function program(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const p = gl.createProgram()!;
  for (const [type, src] of [
    [gl.VERTEX_SHADER, vs],
    [gl.FRAGMENT_SHADER, fs],
  ] as const) {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? 'shader compile failed');
    gl.attachShader(p, s);
  }
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p) ?? 'link failed');
  return p;
}
