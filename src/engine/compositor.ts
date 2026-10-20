// Display compositor: draws the active document with WebGL2 from tiles streamed from the native
// engine. Tiles live in a texture-array atlas addressed through per-source, per-mip page tables.
// While a tile is loading, the shader falls back to coarser mip levels, so the view never shows holes.
//
// Two modes:
// - composite: one source, the engine's flattened image;
// - live: three sources (below / layer / above, see src-tauri/src/doc/live.rs) blended on the GPU,
//   so opacity drags and blend-mode changes show instantly.

import { tileUrl, liveUrl, selUrl } from './bridge';
import { BLEND_GLSL } from './glsl';
import { BLEND_MODES, type BlendMode } from './blend';

export const TILE = 256;
const MAX_LOD = 6;
const LODS = MAX_LOD + 1;
const SOURCES = 5; // 0 composite, 1 below, 2 layer, 3 above, 4 selection
const PARTS = ['', 'below', 'layer', 'above'] as const;
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
  /** Changes only when the selection (or refine preview) changes; 0 = nothing selected. */
  selection: number;
}

/** How the refine-edge workspace shows the refined selection. */
export type RefineView = 'ants' | 'overlay' | 'black' | 'white' | 'mask' | 'layers';
const REFINE_VIEWS: RefineView[] = ['ants', 'overlay', 'black', 'white', 'mask', 'layers'];

export interface LiveState {
  session: number;
  blend: BlendMode;
  /** Layer opacity × fill. */
  alpha: number;
  visible: boolean;
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
uniform vec3 uPasteboard;
uniform bool uPixelGrid;
uniform float uCanvasH;
uniform bool uLive;
uniform int uLiveMode;
uniform float uLiveAlpha;
uniform bool uSel;
uniform vec2 uSelOffset;
uniform vec2 uDx;
uniform vec2 uDy;
uniform float uTime;
uniform int uRefineView;
out vec4 outColor;
${BLEND_GLSL}

// Premultiplied texel of a source at integer lod coordinates. ok=false when not loaded yet.
vec4 texel(int src, ivec2 p, int lod, out bool ok) {
  ivec2 t = p >> 8;
  uint e = texelFetch(uPages, ivec3(t, src * ${LODS} + lod), 0).r;
  if (e == 0u) { ok = false; return vec4(0.0); }
  ok = true;
  if (e == ${EMPTY}u) return vec4(0.0);
  vec4 c = texelFetch(uAtlas, ivec3(p & 255, int(e) - 1), 0);
  return vec4(c.rgb * c.a, c.a);
}

vec4 sampleLod(int src, vec2 doc, int lod, out bool ok) {
  float s = exp2(float(lod));
  ivec2 size = ivec2(ceil(uDoc / s));
  if (lod == 0 && uZoom >= 1.0) {
    return texel(src, clamp(ivec2(floor(doc)), ivec2(0), size - 1), 0, ok);
  }
  vec2 p = doc / s - 0.5;
  ivec2 i0 = ivec2(floor(p));
  vec2 f = p - vec2(i0);
  ivec2 lo = ivec2(0), hi = size - 1;
  bool a, b, c, d;
  vec4 t00 = texel(src, clamp(i0, lo, hi), lod, a);
  vec4 t10 = texel(src, clamp(i0 + ivec2(1, 0), lo, hi), lod, b);
  vec4 t01 = texel(src, clamp(i0 + ivec2(0, 1), lo, hi), lod, c);
  vec4 t11 = texel(src, clamp(i0 + ivec2(1, 1), lo, hi), lod, d);
  ok = a && b && c && d;
  return mix(mix(t00, t10, f.x), mix(t01, t11, f.x), f.y);
}

vec4 sampleSource(int src, vec2 doc) {
  bool ok = false;
  vec4 c = vec4(0.0);
  for (int l = uLod; l <= ${MAX_LOD}; l++) {
    c = sampleLod(src, doc, l, ok);
    if (ok) return c;
  }
  return vec4(0.0);
}

void main() {
  vec2 screen = vec2(gl_FragCoord.x, uCanvasH - gl_FragCoord.y);
  vec2 doc = (uScreenToDoc * vec3(screen, 1.0)).xy;
  if (doc.x < 0.0 || doc.y < 0.0 || doc.x >= uDoc.x || doc.y >= uDoc.y) {
    outColor = vec4(uPasteboard, 1.0);
    return;
  }
  vec4 c;
  if (uLive) {
    c = sampleSource(1, doc);
    vec4 l = sampleSource(2, doc);
    float a = l.a * uLiveAlpha;
    if (uLiveMode == 1) a = dissolveThreshold(ivec2(floor(doc))) < a ? 1.0 : 0.0;
    if (a > 0.0) c = compositePx(uLiveMode, c, l.rgb / max(l.a, 1e-6), a);
    vec4 above = sampleSource(3, doc);
    c = above + c * (1.0 - above.a);
  } else {
    c = sampleSource(0, doc);
  }
  float sel = 1.0;
  if (uSel) sel = sampleSource(4, doc - uSelOffset).a;
  if (uSel && uRefineView == 5) c *= sel; // on layers: unselected becomes transparent
  // transparency checkerboard, fixed in screen space like most editors
  ivec2 q = ivec2(screen) / 8;
  float k = ((q.x + q.y) & 1) == 0 ? 0.80 : 0.66;
  vec3 rgb = c.rgb + vec3(k) * (1.0 - c.a);
  if (uSel) {
    if (uRefineView == 1) rgb = mix(rgb, vec3(1.0, 0.13, 0.13), 0.55 * (1.0 - sel));
    else if (uRefineView == 2) rgb = rgb * sel;
    else if (uRefineView == 3) rgb = rgb * sel + vec3(1.0 - sel);
    else if (uRefineView == 4) rgb = vec3(sel);
  }
  if (uPixelGrid && uZoom >= 8.0) {
    vec2 g = fract(doc) * uZoom;
    if (g.x < 1.0 || g.y < 1.0) rgb = mix(rgb, vec3(0.5), 0.35);
  }
  // marching ants on the 50% edge of the selection, in screen space
  if (uSel && uRefineView == 0) {
    bool inside = sel >= 0.5;
    float sx = sampleSource(4, doc - uSelOffset + uDx).a;
    float sy = sampleSource(4, doc - uSelOffset + uDy).a;
    float sx2 = sampleSource(4, doc - uSelOffset - uDx).a;
    float sy2 = sampleSource(4, doc - uSelOffset - uDy).a;
    if (inside != (sx >= 0.5) || inside != (sy >= 0.5) || inside != (sx2 >= 0.5) || inside != (sy2 >= 0.5)) {
      float dash = mod(floor((screen.x + screen.y) / 4.0 + uTime * 6.0), 2.0);
      rgb = vec3(dash);
    }
  }
  outColor = vec4(rgb, 1.0);
}`;

/** Test shader: composites one tile onto another with the same GLSL used for live previews. */
const FS_BLEND_TEST = `#version 300 es
precision highp float;
precision highp int;
uniform sampler2D uBack;
uniform sampler2D uSrc;
uniform int uMode;
uniform float uOpacity;
out vec4 outColor;
${BLEND_GLSL}
void main() {
  ivec2 p = ivec2(gl_FragCoord.xy);
  vec4 b = texelFetch(uBack, p, 0);
  vec4 s = texelFetch(uSrc, p, 0);
  vec4 dst = vec4(b.rgb * b.a, b.a);
  float a = s.a * uOpacity;
  if (uMode == 1) a = dissolveThreshold(p) < a ? 1.0 : 0.0;
  vec4 c = compositePx(uMode, dst, s.rgb, a);
  outColor = c.a > 0.0 ? vec4(c.rgb / c.a, c.a) : vec4(0.0);
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
  private queue: { key: string; src: number; lod: number; tx: number; ty: number; prio: number }[] = [];
  private queued = new Set<string>();
  private active = 0;
  private frame = 0;
  private generation = 0;
  private drawnVersion = -1;
  private live: LiveState | null = null;
  /** After a live edit is committed: keep drawing live until the new composite is ready. */
  private ending: { session: number; onDone: () => void } | null = null;
  pixelGrid = true;
  /** Preview offset of the selection outline while it is being dragged (document px). */
  selectionOffset: [number, number] = [0, 0];
  refineView: RefineView = 'ants';
  private antsTimer = 0;
  pasteboard: [number, number, number] = [0.086, 0.09, 0.106];
  /** Called after each drawn frame (for overlays, perf). */
  onFrame: ((ms: number) => void) | null = null;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext('webgl2', { antialias: false, alpha: false, depth: false, stencil: false, desynchronized: true, powerPreference: 'high-performance' });
    if (!gl) throw new Error('WebGL2 is not available');
    this.gl = gl;
    this.prog = program(gl, VS, FS);
    for (const n of ['uAtlas', 'uPages', 'uScreenToDoc', 'uDoc', 'uZoom', 'uLod', 'uPasteboard', 'uPixelGrid', 'uCanvasH', 'uLive', 'uLiveMode', 'uLiveAlpha', 'uSel', 'uSelOffset', 'uDx', 'uDy', 'uTime', 'uRefineView']) {
      this.u[n] = gl.getUniformLocation(this.prog, n);
    }
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
    this.live = null;
    this.ending = null;
  }

  setDocument(doc: DisplayDoc | null) {
    const gl = this.gl;
    const same = this.doc && doc && this.doc.id === doc.id && this.doc.width === doc.width && this.doc.height === doc.height;
    if (same) {
      if (doc!.version !== this.doc!.version || doc!.selection !== this.doc!.selection) {
        if (doc!.selection !== this.doc!.selection && !doc!.selection) this.dropSources([4]);
        this.doc = { ...doc! };
        this.animateAnts();
        this.requestFrame(); // stale tiles stay visible until their replacements arrive
      }
      return;
    }
    this.doc = doc ? { ...doc } : null;
    this.resetAtlas();
    if (!doc) return this.requestFrame();
    this.pageW = Math.ceil(doc.width / TILE);
    this.pageH = Math.ceil(doc.height / TILE);
    this.pageData = Array.from({ length: LODS * SOURCES }, () => new Uint16Array(this.pageW * this.pageH));
    gl.deleteTexture(this.pages);
    this.pages = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.pages);
    gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.R16UI, this.pageW, this.pageH, LODS * SOURCES);
    nearest(gl, gl.TEXTURE_2D_ARRAY);
    for (let l = 0; l < LODS * SOURCES; l++) this.pagesDirty.add(l);
    this.animateAnts();
    this.requestFrame();
  }

  /** Marching ants move while there is a selection. */
  private animateAnts() {
    const want = !!this.doc?.selection;
    if (want && !this.antsTimer) this.antsTimer = window.setInterval(() => this.requestFrame(), 120);
    if (!want && this.antsTimer) {
      clearInterval(this.antsTimer);
      this.antsTimer = 0;
    }
  }

  setView(view: ViewTransform) {
    this.view = { ...view };
    this.requestFrame();
  }

  /** Starts or updates a live preview. */
  setLive(live: LiveState) {
    if (this.live && this.live.session !== live.session) this.dropSources([1, 2, 3]);
    this.live = { ...live };
    this.ending = null;
    this.requestFrame();
  }

  /** The live edit was committed: switch back to the composite once it is fully loaded. */
  endLive(onDone: () => void) {
    if (!this.live) return onDone();
    this.ending = { session: this.live.session, onDone };
    this.requestFrame();
  }

  /** Abandons a live preview immediately. */
  cancelLive() {
    if (!this.live) return;
    this.live = null;
    this.ending = null;
    this.dropSources([1, 2, 3]);
    this.requestFrame();
  }

  get liveSession(): number | null {
    return this.live?.session ?? null;
  }

  private dropSources(srcs: number[]) {
    for (const [key, e] of [...this.entries]) {
      const src = Number(key.split('/')[0]);
      if (!srcs.includes(src)) continue;
      if (e.slot !== EMPTY) this.release(e.slot);
      this.entries.delete(key);
    }
    for (const src of srcs)
      for (let l = 0; l < LODS; l++) {
        this.pageData[src * LODS + l]?.fill(0);
        this.pagesDirty.add(src * LODS + l);
      }
  }

  resize(cssW: number, cssH: number, dpr: number) {
    const w = Math.max(1, Math.round(cssW * dpr));
    const h = Math.max(1, Math.round(cssH * dpr));
    if (w === this.w && h === this.h) return;
    this.canvas.width = this.w = w;
    this.canvas.height = this.h = h;
    this.requestFrame();
  }

  /** Coarsest useful lod: the whole document in at most 2×2 tiles, always kept loaded. */
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
    return [cos, sin, -sin, cos, cx - (cos * ox - sin * oy), cy - (sin * ox + cos * oy)];
  }

  requestFrame() {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.draw();
    });
  }

  private visibleTiles(lod: number) {
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
    return {
      tx0: Math.max(0, Math.floor(Math.min(...xs) / span)),
      ty0: Math.max(0, Math.floor(Math.min(...ys) / span)),
      tx1: Math.min(Math.ceil(this.doc!.width / span) - 1, Math.floor(Math.max(...xs) / span)),
      ty1: Math.min(Math.ceil(this.doc!.height / span) - 1, Math.floor(Math.max(...ys) / span)),
    };
  }

  private versionOf(src: number): number {
    if (src === 0) return this.doc!.version;
    if (src === 4) return this.doc!.selection;
    return this.live!.session;
  }

  /** Requests a tile; returns true if the current version is already loaded. */
  private want(src: number, lod: number, tx: number, ty: number, prio: number): boolean {
    const key = `${src}/${lod}/${tx}/${ty}`;
    const e = this.entries.get(key);
    if (e && e.slot !== EMPTY) {
      this.lru.delete(e.slot);
      this.lru.set(e.slot, key);
    }
    if (e && e.version === this.versionOf(src)) return true;
    if (!this.inflight.has(key) && !this.queued.has(key)) {
      this.queued.add(key);
      this.queue.push({ key, src, lod, tx, ty, prio });
    }
    return false;
  }

  private pump() {
    this.queue.sort((a, b) => a.prio - b.prio);
    while (this.active < CONCURRENCY && this.queue.length) {
      const job = this.queue.shift()!;
      if (this.inflight.has(job.key)) continue;
      this.inflight.add(job.key);
      this.active++;
      const version = this.versionOf(job.src);
      const url =
        job.src === 0
          ? tileUrl(this.doc!.id, version, job.lod, job.tx, job.ty)
          : job.src === 4
            ? selUrl(this.doc!.id, version, job.lod, job.tx, job.ty)
            : liveUrl(version, PARTS[job.src] as 'below', job.lod, job.tx, job.ty);
      const gen = this.generation;
      fetch(url)
        .then(async (r) => (r.status === 200 ? new Uint8Array(await r.arrayBuffer()) : r.status === 204 ? null : Promise.reject(r.status)))
        .then((data) => {
          const current = job.src === 0 || (job.src === 4 ? this.doc?.selection === version : this.live?.session === version);
          if (gen === this.generation && current) this.store(job.key, job.src, job.lod, job.tx, job.ty, version, data);
        })
        .catch(() => {})
        .finally(() => {
          if (gen === this.generation) this.inflight.delete(job.key);
          this.active--;
          this.requestFrame();
        });
    }
  }

  private store(key: string, src: number, lod: number, tx: number, ty: number, version: number, data: Uint8Array | null) {
    const gl = this.gl;
    const prev = this.entries.get(key);
    if (prev && (src === 0) && prev.version > version) return;
    let slot: number;
    if (data === null) {
      if (prev && prev.slot !== EMPTY) this.release(prev.slot);
      slot = EMPTY;
    } else {
      slot = prev && prev.slot !== EMPTY ? prev.slot : this.allocate();
      gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlas);
      gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, 0, 0, slot - 1, TILE, TILE, 1, gl.RGBA, gl.UNSIGNED_BYTE, data);
      this.lru.delete(slot);
      this.lru.set(slot, key);
    }
    this.entries.set(key, { slot, version });
    this.pageData[src * LODS + lod][ty * this.pageW + tx] = slot;
    this.pagesDirty.add(src * LODS + lod);
  }

  private allocate(): number {
    const s = this.free.pop();
    if (s !== undefined) return s;
    const [slot, key] = this.lru.entries().next().value as [number, string];
    this.lru.delete(slot);
    const [src, lod, tx, ty] = key.split('/').map(Number);
    this.entries.delete(key);
    this.pageData[src * LODS + lod][ty * this.pageW + tx] = 0;
    this.pagesDirty.add(src * LODS + lod);
    return slot;
  }

  private release(slot: number) {
    this.lru.delete(slot);
    this.free.push(slot);
  }

  /** Requests the base level and the visible tiles of a source; true when all are current. */
  private request(src: number, lod: number, base: number): boolean {
    let complete = true;
    const span = TILE << base;
    for (let ty = 0; ty * span < this.doc!.height; ty++) for (let tx = 0; tx * span < this.doc!.width; tx++) complete = this.want(src, base, tx, ty, -1e9) && complete;
    if (lod < base) {
      const v = this.visibleTiles(lod);
      const sp = TILE << lod;
      for (let ty = v.ty0; ty <= v.ty1; ty++)
        for (let tx = v.tx0; tx <= v.tx1; tx++) {
          const d = Math.hypot((tx + 0.5) * sp - this.view.cx, (ty + 0.5) * sp - this.view.cy);
          complete = this.want(src, lod, tx, ty, d + src) && complete;
        }
    }
    return complete;
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
    this.queue = [];
    this.queued.clear();
    if (this.live) {
      for (const src of [1, 2, 3]) this.request(src, lod, base);
      if (this.ending && this.request(0, lod, base)) {
        // committed: the new composite is loaded, switch back to it
        const done = this.ending.onDone;
        this.live = null;
        this.ending = null;
        this.dropSources([1, 2, 3]);
        done();
      }
    } else {
      this.request(0, lod, base);
    }
    if (this.doc.selection) this.request(4, lod, base);
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
    gl.uniform3f(this.u.uPasteboard, ...this.pasteboard);
    gl.uniform1i(this.u.uPixelGrid, this.pixelGrid ? 1 : 0);
    gl.uniform1f(this.u.uCanvasH, this.h);
    gl.uniform1i(this.u.uLive, this.live ? 1 : 0);
    gl.uniform1i(this.u.uLiveMode, this.live ? BLEND_MODES.indexOf(this.live.blend) : 0);
    gl.uniform1f(this.u.uLiveAlpha, this.live && this.live.visible ? this.live.alpha : 0);
    gl.uniform1i(this.u.uSel, this.doc.selection ? 1 : 0);
    gl.uniform2f(this.u.uSelOffset, ...this.selectionOffset);
    gl.uniform2f(this.u.uDx, m[0] * 1.5, m[1] * 1.5);
    gl.uniform2f(this.u.uDy, m[2] * 1.5, m[3] * 1.5);
    gl.uniform1f(this.u.uTime, (performance.now() / 1000) % 1000);
    gl.uniform1i(this.u.uRefineView, REFINE_VIEWS.indexOf(this.refineView));
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlas);
    gl.uniform1i(this.u.uAtlas, 0);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.pages);
    gl.uniform1i(this.u.uPages, 1);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    if (!this.live) this.drawnVersion = this.doc.version;
    this.onFrame?.(performance.now() - t0);
  }

  /** True when every tile needed for the current view at full detail is loaded and drawn. */
  settled(): boolean {
    return this.frame === 0 && this.queue.length === 0 && this.inflight.size === 0 && !this.live && this.drawnVersion === this.doc?.version;
  }

  /** Tiles still wanted for the current view (queued or loading). */
  pending(): number {
    return this.queue.length + this.inflight.size;
  }

  /**
   * GPU-vs-CPU check: composites `src` onto `back` (256×256 straight RGBA8 each) with the live
   * shader's GLSL and returns the straight RGBA8 result.
   */
  blendTest(back: Uint8Array, src: Uint8Array, mode: BlendMode, opacity: number): Uint8Array {
    const gl = this.gl;
    const prog = program(gl, VS, FS_BLEND_TEST);
    const tex = (data: Uint8Array) => {
      const t = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, TILE, TILE, 0, gl.RGBA, gl.UNSIGNED_BYTE, data);
      nearest(gl, gl.TEXTURE_2D);
      return t;
    };
    const tb = tex(back);
    const ts = tex(src);
    const out = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, out);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA8, TILE, TILE);
    const fb = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fb);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, out, 0);
    gl.viewport(0, 0, TILE, TILE);
    gl.useProgram(prog);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, tb);
    gl.uniform1i(gl.getUniformLocation(prog, 'uBack'), 0);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, ts);
    gl.uniform1i(gl.getUniformLocation(prog, 'uSrc'), 1);
    gl.uniform1i(gl.getUniformLocation(prog, 'uMode'), BLEND_MODES.indexOf(mode));
    gl.uniform1f(gl.getUniformLocation(prog, 'uOpacity'), opacity);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    const result = new Uint8Array(TILE * TILE * 4);
    gl.readPixels(0, 0, TILE, TILE, gl.RGBA, gl.UNSIGNED_BYTE, result);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    for (const t of [tb, ts, out]) gl.deleteTexture(t);
    gl.deleteFramebuffer(fb);
    gl.deleteProgram(prog);
    this.requestFrame();
    return result;
  }

  destroy() {
    cancelAnimationFrame(this.frame);
    clearInterval(this.antsTimer);
    this.generation++;
    this.gl.getExtension('WEBGL_lose_context')?.loseContext();
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
