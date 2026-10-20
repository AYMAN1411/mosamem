// Milestone 0 spike: tiled, virtual-textured compositing of many large layers in WebGL2.
// Each layer pass reads the accumulated backdrop and writes a blended result (ping-pong in RGBA16F).

export const TILE = 256;
const SLOT = TILE + 2; // 1 px gutter so bilinear filtering stays inside a tile
const MIPS = 4;

export interface GpuSpikeOptions {
  canvas: HTMLCanvasElement;
  docW: number;
  docH: number;
  layers: number;
}

const VS = `#version 300 es
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

const FS_LAYER = `#version 300 es
precision highp float;
precision highp int;
precision highp usampler2DArray;
precision highp sampler2DArray;
uniform sampler2DArray uAtlas;
uniform usampler2DArray uPages;
uniform sampler2D uBackdrop;
uniform mat3 uScreenToDoc;
uniform vec2 uDoc;
uniform int uLayer;
uniform int uLod;
uniform int uMode;
uniform float uOpacity;
out vec4 outColor;

vec3 blend(int m, vec3 b, vec3 s) {
  if (m == 1) return b * s;
  if (m == 2) return b + s - b * s;
  if (m == 3) { // overlay = hardlight(s, b)
    return mix(2.0 * b * s, 1.0 - 2.0 * (1.0 - b) * (1.0 - s), step(0.5, b));
  }
  return s;
}

void main() {
  vec4 back = texelFetch(uBackdrop, ivec2(gl_FragCoord.xy), 0);
  vec2 doc = (uScreenToDoc * vec3(gl_FragCoord.xy, 1.0)).xy;
  if (doc.x < 0.0 || doc.y < 0.0 || doc.x >= uDoc.x || doc.y >= uDoc.y) { outColor = back; return; }
  float scale = exp2(float(uLod));
  vec2 p = doc / scale;
  ivec2 t = ivec2(floor(p / ${TILE}.0));
  uint slot = texelFetch(uPages, ivec3(t, uLayer * ${MIPS} + uLod), 0).r;
  if (slot == 0u) { outColor = back; return; }
  vec2 local = p - vec2(t) * ${TILE}.0;
  vec4 s = texture(uAtlas, vec3((local + 1.0) / ${SLOT}.0, float(slot - 1u))); // premultiplied
  s *= uOpacity;
  // W3C general formula, premultiplied: co = cs(1-ab) + cb(1-as) + as*ab*B(Cb,Cs)
  vec3 Cs = s.a > 0.0 ? s.rgb / s.a : vec3(0.0);
  vec3 Cb = back.a > 0.0 ? back.rgb / back.a : vec3(0.0);
  vec3 co = s.rgb * (1.0 - back.a) + back.rgb * (1.0 - s.a) + s.a * back.a * blend(uMode, Cb, Cs);
  outColor = vec4(co, s.a + back.a * (1.0 - s.a));
}`;

const FS_PRESENT = `#version 300 es
precision highp float;
uniform sampler2D uImage;
out vec4 outColor;
void main() {
  ivec2 c = ivec2(gl_FragCoord.xy);
  vec4 img = texelFetch(uImage, c, 0);
  float k = ((c.x >> 3) + (c.y >> 3)) % 2 == 0 ? 0.8 : 0.6;
  outColor = vec4(img.rgb + vec3(k) * (1.0 - img.a), 1.0);
}`;

function compile(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const prog = gl.createProgram()!;
  for (const [type, src] of [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]] as const) {
    const sh = gl.createShader(type)!;
    gl.shaderSource(sh, src);
    gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(sh) ?? 'shader');
    gl.attachShader(prog, sh);
  }
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog) ?? 'link');
  return prog;
}

/** Procedural layer content: a translucent tinted disc per layer, row-filled for speed. */
export function fillTile(out: Uint32Array, layer: number, lod: number, tx: number, ty: number, docW: number, docH: number) {
  const s = 1 << lod;
  const cx = ((layer * 1931) % 97) / 97 * docW;
  const cy = ((layer * 3571) % 89) / 89 * docH;
  const r = (0.15 + ((layer * 7) % 10) / 40) * Math.min(docW, docH);
  const hue = (layer * 47) % 360;
  const [cr, cg, cb] = hsv(hue, 0.7, 0.9);
  const a = layer === 0 ? 255 : 160;
  out.fill(0);
  for (let row = 0; row < TILE; row++) {
    const y = (ty * TILE + row) * s;
    if (layer === 0) {
      // opaque background with a vertical gradient
      const g = Math.round(40 + (y / docH) * 120);
      out.subarray(row * TILE, row * TILE + TILE).fill(pack(g, g, g + 20, 255));
      continue;
    }
    const dy = y - cy;
    if (Math.abs(dy) >= r) continue;
    const half = Math.sqrt(r * r - dy * dy);
    const x0 = Math.max(0, Math.ceil((cx - half) / s) - tx * TILE);
    const x1 = Math.min(TILE, Math.floor((cx + half) / s) - tx * TILE);
    if (x1 <= x0) continue;
    const shade = 0.75 + 0.25 * (1 - Math.abs(dy) / r);
    const pa = a / 255;
    out.subarray(row * TILE + x0, row * TILE + x1).fill(
      pack(Math.round(cr * shade * pa), Math.round(cg * shade * pa), Math.round(cb * shade * pa), a),
    );
  }
}

function pack(r: number, g: number, b: number, a: number) {
  return ((a << 24) | (b << 16) | (g << 8) | r) >>> 0;
}

function hsv(h: number, s: number, v: number): [number, number, number] {
  const f = (n: number) => {
    const k = (n + h / 60) % 6;
    return 255 * (v - v * s * Math.max(0, Math.min(k, 4 - k, 1)));
  };
  return [f(5), f(3), f(1)];
}

export class GpuSpike {
  gl: WebGL2RenderingContext;
  docW: number;
  docH: number;
  layers: number;
  tilesX: number;
  tilesY: number;
  capacity: number;
  atlas: WebGLTexture;
  pages: WebGLTexture;
  pageData: Uint16Array[]; // per (layer, lod)
  pageDirty = new Set<number>();
  slotOwner: Int32Array; // slot -> key
  lru: Map<number, number> = new Map(); // key -> slot (insertion order = LRU)
  freeSlots: number[] = [];
  fb: WebGLFramebuffer[] = [];
  accum: WebGLTexture[] = [];
  progLayer: WebGLProgram;
  progPresent: WebGLProgram;
  scratch = new Uint32Array(TILE * TILE);
  uploadsThisFrame = 0;
  missingThisFrame = 0;
  uploadBudget = 96;
  w = 0;
  h = 0;

  constructor(o: GpuSpikeOptions) {
    const gl = o.canvas.getContext('webgl2', { antialias: false, alpha: false, premultipliedAlpha: true, desynchronized: true, powerPreference: 'high-performance' });
    if (!gl) throw new Error('WebGL2 unavailable');
    this.gl = gl;
    if (!gl.getExtension('EXT_color_buffer_float') && !gl.getExtension('EXT_color_buffer_half_float')) throw new Error('no float color buffers');
    this.docW = o.docW;
    this.docH = o.docH;
    this.layers = o.layers;
    this.tilesX = Math.ceil(o.docW / TILE);
    this.tilesY = Math.ceil(o.docH / TILE);
    this.capacity = Math.min(gl.getParameter(gl.MAX_ARRAY_TEXTURE_LAYERS) as number, 2048);

    this.atlas = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlas);
    gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.RGBA8, SLOT, SLOT, this.capacity);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    for (let i = this.capacity; i >= 1; i--) this.freeSlots.push(i);
    this.slotOwner = new Int32Array(this.capacity + 1).fill(-1);

    this.pages = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.pages);
    gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.R16UI, this.tilesX, this.tilesY, this.layers * MIPS);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    this.pageData = [];
    for (let i = 0; i < this.layers * MIPS; i++) {
      this.pageData.push(new Uint16Array(this.tilesX * this.tilesY));
      this.pageDirty.add(i);
    }
    this.progLayer = compile(gl, VS, FS_LAYER);
    this.progPresent = compile(gl, VS, FS_PRESENT);
    gl.bindVertexArray(gl.createVertexArray());
  }

  resize(w: number, h: number) {
    const gl = this.gl;
    if (w === this.w && h === this.h) return;
    this.w = w;
    this.h = h;
    for (const t of this.accum) gl.deleteTexture(t);
    for (const f of this.fb) gl.deleteFramebuffer(f);
    this.accum = [];
    this.fb = [];
    for (let i = 0; i < 2; i++) {
      const t = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA16F, w, h);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
      const f = gl.createFramebuffer()!;
      gl.bindFramebuffer(gl.FRAMEBUFFER, f);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, t, 0);
      this.accum.push(t);
      this.fb.push(f);
    }
  }

  private key(layer: number, lod: number, tx: number, ty: number) {
    return ((layer * MIPS + lod) * this.tilesY + ty) * this.tilesX + tx;
  }

  /** Makes a tile resident; returns false when the per-frame upload budget is spent. */
  private ensure(layer: number, lod: number, tx: number, ty: number): boolean {
    const k = this.key(layer, lod, tx, ty);
    const slot = this.lru.get(k);
    if (slot !== undefined) {
      this.lru.delete(k);
      this.lru.set(k, slot);
      return true;
    }
    if (this.uploadsThisFrame >= this.uploadBudget) {
      this.missingThisFrame++;
      return false;
    }
    let s = this.freeSlots.pop();
    if (s === undefined) {
      const [oldKey, oldSlot] = this.lru.entries().next().value as [number, number];
      this.lru.delete(oldKey);
      this.unmap(oldKey);
      s = oldSlot;
    }
    fillTile(this.scratch, layer, lod, tx, ty, this.docW, this.docH);
    const gl = this.gl;
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlas);
    gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, 1, 1, s - 1, TILE, TILE, 1, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(this.scratch.buffer));
    this.uploadsThisFrame++;
    this.lru.set(k, s);
    this.slotOwner[s] = k;
    const plane = layer * MIPS + lod;
    this.pageData[plane][ty * this.tilesX + tx] = s;
    this.pageDirty.add(plane);
    return true;
  }

  private unmap(k: number) {
    const tx = k % this.tilesX;
    const r = Math.floor(k / this.tilesX);
    const ty = r % this.tilesY;
    const plane = Math.floor(r / this.tilesY);
    this.pageData[plane][ty * this.tilesX + tx] = 0;
    this.pageDirty.add(plane);
  }

  /** zoom = screen px per doc px; (cx, cy) = doc point at the canvas center. */
  frame(zoom: number, cx: number, cy: number) {
    const gl = this.gl;
    this.uploadsThisFrame = 0;
    this.missingThisFrame = 0;
    const lod = Math.max(0, Math.min(MIPS - 1, Math.floor(Math.log2(1 / zoom))));
    const scale = 1 << lod;
    // visible doc rect (margin of one tile for prefetch)
    const halfW = this.w / 2 / zoom;
    const halfH = this.h / 2 / zoom;
    const t = TILE * scale;
    const tx0 = Math.max(0, Math.floor((cx - halfW) / t) - 1);
    const ty0 = Math.max(0, Math.floor((cy - halfH) / t) - 1);
    const tx1 = Math.min(Math.ceil(this.docW / t) - 1, Math.floor((cx + halfW) / t) + 1);
    const ty1 = Math.min(Math.ceil(this.docH / t) - 1, Math.floor((cy + halfH) / t) + 1);
    for (let l = 0; l < this.layers; l++)
      for (let ty = ty0; ty <= ty1; ty++) for (let tx = tx0; tx <= tx1; tx++) this.ensure(l, lod, tx, ty);

    if (this.pageDirty.size) {
      gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.pages);
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 2);
      for (const plane of this.pageDirty)
        gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, 0, 0, plane, this.tilesX, this.tilesY, 1, gl.RED_INTEGER, gl.UNSIGNED_SHORT, this.pageData[plane]);
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 4);
      this.pageDirty.clear();
    }

    // screen (gl_FragCoord, y up) -> doc (y down)
    const m = new Float32Array([1 / zoom, 0, 0, 0, -1 / zoom, 0, cx - halfW, cy + halfH, 1]);
    gl.viewport(0, 0, this.w, this.h);
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.fb[0]);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const p = this.progLayer;
    gl.useProgram(p);
    gl.uniformMatrix3fv(gl.getUniformLocation(p, 'uScreenToDoc'), false, m);
    gl.uniform2f(gl.getUniformLocation(p, 'uDoc'), this.docW, this.docH);
    gl.uniform1i(gl.getUniformLocation(p, 'uLod'), lod);
    gl.uniform1i(gl.getUniformLocation(p, 'uAtlas'), 0);
    gl.uniform1i(gl.getUniformLocation(p, 'uPages'), 1);
    gl.uniform1i(gl.getUniformLocation(p, 'uBackdrop'), 2);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlas);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.pages);
    const uLayer = gl.getUniformLocation(p, 'uLayer');
    const uMode = gl.getUniformLocation(p, 'uMode');
    const uOpacity = gl.getUniformLocation(p, 'uOpacity');
    let src = 0;
    for (let l = 0; l < this.layers; l++) {
      gl.activeTexture(gl.TEXTURE2);
      gl.bindTexture(gl.TEXTURE_2D, this.accum[src]);
      gl.bindFramebuffer(gl.FRAMEBUFFER, this.fb[1 - src]);
      gl.uniform1i(uLayer, l);
      gl.uniform1i(uMode, l === 0 ? 0 : l % 4);
      gl.uniform1f(uOpacity, 1);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      src = 1 - src;
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.useProgram(this.progPresent);
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_2D, this.accum[src]);
    gl.uniform1i(gl.getUniformLocation(this.progPresent, 'uImage'), 2);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    return { lod, uploads: this.uploadsThisFrame, missing: this.missingThisFrame, resident: this.lru.size };
  }
}
