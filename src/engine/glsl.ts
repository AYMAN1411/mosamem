// GLSL port of lawha-core's blend math (crates/lawha-core/src/blend.rs). Mode indices follow
// BlendMode::ALL. Kept line-for-line close to the Rust so the GPU-vs-CPU test can hold ±1/255.
export const BLEND_GLSL = `
float clamp01(float x) { return clamp(x, 0.0, 1.0); }
float hardLight1(float cb, float cs) {
  if (cs <= 0.5) return cb * 2.0 * cs;
  float s = 2.0 * cs - 1.0;
  return cb + s - cb * s;
}
float colorDodge1(float cb, float cs) {
  if (cb <= 0.0) return 0.0;
  if (cs >= 1.0) return 1.0;
  return min(cb / (1.0 - cs), 1.0);
}
float colorBurn1(float cb, float cs) {
  if (cb >= 1.0) return 1.0;
  if (cs <= 0.0) return 0.0;
  return 1.0 - min((1.0 - cb) / cs, 1.0);
}
float softLight1(float cb, float cs) {
  if (cs < 0.5) return 2.0 * cb * cs + cb * cb * (1.0 - 2.0 * cs);
  return 2.0 * cb * (1.0 - cs) + sqrt(cb) * (2.0 * cs - 1.0);
}
float blend1(int m, float cb, float cs) {
  if (m == 0 || m == 1) return cs;
  if (m == 2) return min(cb, cs);
  if (m == 3) return cb * cs;
  if (m == 4) return colorBurn1(cb, cs);
  if (m == 5) return clamp01(cb + cs - 1.0);
  if (m == 7) return max(cb, cs);
  if (m == 8) return cb + cs - cb * cs;
  if (m == 9) return colorDodge1(cb, cs);
  if (m == 10) return clamp01(cb + cs);
  if (m == 12) return hardLight1(cs, cb);
  if (m == 13) return softLight1(cb, cs);
  if (m == 14) return hardLight1(cb, cs);
  if (m == 15) return cs < 0.5 ? colorBurn1(cb, 2.0 * cs) : colorDodge1(cb, 2.0 * cs - 1.0);
  if (m == 16) return clamp01(cb + 2.0 * cs - 1.0);
  if (m == 17) return cs <= 0.5 ? min(cb, 2.0 * cs) : max(cb, 2.0 * cs - 1.0);
  if (m == 18) return cb + cs >= 1.0 ? 1.0 : 0.0;
  if (m == 19) return abs(cb - cs);
  if (m == 20) return cb + cs - 2.0 * cb * cs;
  if (m == 21) return clamp01(cb - cs);
  if (m == 22) { if (cs <= 0.0) return cb <= 0.0 ? 0.0 : 1.0; return clamp01(cb / cs); }
  return cs;
}
float lum(vec3 c) { return 0.3 * c.r + 0.59 * c.g + 0.11 * c.b; }
vec3 clipColor(vec3 c) {
  float l = lum(c);
  float n = min(min(c.r, c.g), c.b);
  float x = max(max(c.r, c.g), c.b);
  if (n < 0.0) c = l + (c - l) * l / (l - n);
  if (x > 1.0) c = l + (c - l) * (1.0 - l) / (x - l);
  return c;
}
vec3 setLum(vec3 c, float l) { return clipColor(c + (l - lum(c))); }
float sat(vec3 c) { return max(max(c.r, c.g), c.b) - min(min(c.r, c.g), c.b); }
vec3 setSat(vec3 c, float s) {
  // order the channels: lo, mid, hi (ties broken the same way as the Rust sort)
  int lo = 0; int mid = 1; int hi = 2;
  if (c[mid] < c[lo]) { int t = lo; lo = mid; mid = t; }
  if (c[hi] < c[mid]) { int t = mid; mid = hi; hi = t; }
  if (c[mid] < c[lo]) { int t = lo; lo = mid; mid = t; }
  vec3 o = vec3(0.0);
  if (c[hi] > c[lo]) {
    o[mid] = (c[mid] - c[lo]) * s / (c[hi] - c[lo]);
    o[hi] = s;
  }
  o[lo] = 0.0;
  return o;
}
vec3 blendRGB(int m, vec3 cb, vec3 cs) {
  if (m == 23) return setLum(setSat(cs, sat(cb)), lum(cb));
  if (m == 24) return setLum(setSat(cb, sat(cs)), lum(cb));
  if (m == 25) return setLum(cs, lum(cb));
  if (m == 26) return setLum(cb, lum(cs));
  if (m == 6) return lum(cs) < lum(cb) ? cs : cb;
  if (m == 11) return lum(cs) > lum(cb) ? cs : cb;
  return vec3(blend1(m, cb.r, cs.r), blend1(m, cb.g, cs.g), blend1(m, cb.b, cs.b));
}
// same 32-bit hash as lawha_core::blend::dissolve_threshold
float dissolveThreshold(ivec2 p) {
  uint h = uint(p.x) * 0x8da6b343u ^ uint(p.y) * 0xd8163841u;
  h ^= h >> 13u;
  h *= 0x5bd1e995u;
  h ^= h >> 15u;
  return float(h >> 8u) / 16777216.0;
}
// W3C general formula, source-over; dst is premultiplied, cs straight, a = source alpha × opacity
vec4 compositePx(int m, vec4 dst, vec3 cs, float a) {
  if (a <= 0.0) return dst;
  float ab = dst.a;
  vec3 cb = ab > 0.0 ? dst.rgb / ab : vec3(0.0);
  vec3 b = (ab > 0.0 && m != 0) ? blendRGB(m, cb, cs) : cs;
  vec3 co = a * (1.0 - ab) * cs + a * ab * b + (1.0 - a) * dst.rgb;
  return vec4(co, a + ab * (1.0 - a));
}
`;

/**
 * Adjustments, as in lawha-core/src/adjust.rs (`Op::apply` and `apply_to_accum`), on the numbers
 * the engine compiles (`adjustment_compile`). Needs BLEND_GLSL before it.
 */
export const ADJUST_GLSL = `
uniform int uAdjKind; // 0 = none, else lawha_core::adjust::OpKind
uniform vec4 uAdjP[8];
uniform highp sampler2D uAdjTable;
uniform int uAdjN;

float adjLookup(float x, int ch) {
  float p = clamp(x, 0.0, 1.0) * float(uAdjN - 1);
  int i = min(int(p), uAdjN - 2);
  float f = p - float(i);
  float a = texelFetch(uAdjTable, ivec2(i, 0), 0)[ch];
  float b = texelFetch(uAdjTable, ivec2(i + 1, 0), 0)[ch];
  return a + (b - a) * f;
}
vec3 rgbToHsl(vec3 c) {
  float mx = max(c.r, max(c.g, c.b));
  float mn = min(c.r, min(c.g, c.b));
  float l = (mx + mn) / 2.0;
  float d = mx - mn;
  if (d <= 1e-6) return vec3(0.0, 0.0, l);
  float s = d / max(1.0 - abs(2.0 * l - 1.0), 1e-6);
  float h;
  if (mx == c.r) h = mod((c.g - c.b) / d, 6.0);
  else if (mx == c.g) h = (c.b - c.r) / d + 2.0;
  else h = (c.r - c.g) / d + 4.0;
  return vec3(h * 60.0, min(s, 1.0), l);
}
vec3 hslToRgb(float h, float s, float l) {
  float c = (1.0 - abs(2.0 * l - 1.0)) * s;
  float hp = mod(h, 360.0) / 60.0;
  float x = c * (1.0 - abs(mod(hp, 2.0) - 1.0));
  int seg = int(hp);
  vec3 rgb = seg == 0 ? vec3(c, x, 0.0) : seg == 1 ? vec3(x, c, 0.0) : seg == 2 ? vec3(0.0, c, x) : seg == 3 ? vec3(0.0, x, c) : seg == 4 ? vec3(x, 0.0, c) : vec3(c, 0.0, x);
  return rgb + (l - c / 2.0);
}
float hueDistance(float a, float b) {
  float d = mod(a - b, 360.0);
  return min(d, 360.0 - d);
}
float rangeWeight(float h, float center) { return clamp((45.0 - hueDistance(h, center)) / 30.0, 0.0, 1.0); }
vec3 lighten(vec3 c, float dl) { return dl >= 0.0 ? c + (1.0 - c) * dl : c * (1.0 + dl); }
vec3 adjHueSaturation(vec3 c) {
  vec3 hsl = rgbToHsl(c);
  if (uAdjP[0].w > 0.5) {
    vec4 k = uAdjP[7];
    return lighten(hslToRgb(k.x, clamp(k.y, 0.0, 1.0), hsl.z), k.z);
  }
  float dh = uAdjP[0].x, ds = uAdjP[0].y, dl = uAdjP[0].z;
  if (hsl.y > 0.0) {
    for (int i = 0; i < 6; i++) {
      float w = rangeWeight(hsl.x, float(i) * 60.0);
      dh += w * uAdjP[i + 1].x;
      ds += w * uAdjP[i + 1].y;
      dl += w * uAdjP[i + 1].z;
    }
  }
  float s2 = clamp(hsl.y * (1.0 + clamp(ds, -1.0, 1.0)), 0.0, 1.0);
  return lighten(hslToRgb(hsl.x + dh, s2, hsl.z), clamp(dl, -1.0, 1.0));
}
vec3 adjVibrance(vec3 c) {
  float v = uAdjP[0].x, sat = uAdjP[0].y;
  vec3 hsl = rgbToHsl(c);
  float skin = v > 0.0 ? 1.0 - 0.5 * max(1.0 - hueDistance(hsl.x, 25.0) / 35.0, 0.0) : 1.0;
  float s1 = clamp(hsl.y * (1.0 + v * (1.0 - hsl.y) * skin), 0.0, 1.0);
  return hslToRgb(hsl.x, clamp(s1 * (1.0 + sat), 0.0, 1.0), hsl.z);
}
vec3 adjColorBalance(vec3 c) {
  float l = clamp(lum(c), 0.0, 1.0);
  vec3 w = vec3((1.0 - l) * (1.0 - l), 2.0 * l * (1.0 - l), l * l);
  vec3 shift = w.x * uAdjP[0].xyz + w.y * uAdjP[1].xyz + w.z * uAdjP[2].xyz;
  vec3 o = clamp(c + 0.5 * shift, 0.0, 1.0);
  return uAdjP[3].x > 0.5 ? setLum(o, lum(c)) : o;
}
float bwWeight(int i) { return i < 4 ? uAdjP[0][i] : uAdjP[1][i - 4]; }
vec3 adjBlackWhite(vec3 c) {
  float r = c.r, g = c.g, b = c.b;
  float mx, mid, mn;
  int pr, sc;
  if (r >= g && r >= b) {
    if (g >= b) { mx = r; mid = g; mn = b; pr = 0; sc = 1; } else { mx = r; mid = b; mn = g; pr = 0; sc = 5; }
  } else if (g >= r && g >= b) {
    if (r >= b) { mx = g; mid = r; mn = b; pr = 2; sc = 1; } else { mx = g; mid = b; mn = r; pr = 2; sc = 3; }
  } else if (r >= g) { mx = b; mid = r; mn = g; pr = 4; sc = 5; } else { mx = b; mid = g; mn = r; pr = 4; sc = 3; }
  float gray = clamp(mn + (mid - mn) * bwWeight(sc) + (mx - mid) * bwWeight(pr), 0.0, 1.0);
  return uAdjP[1].z > 0.5 ? setLum(uAdjP[2].xyz, gray) : vec3(gray);
}
vec3 applyAdjust(vec3 c, ivec2 px) {
  if (uAdjKind == 1) return vec3(adjLookup(c.r, 0), adjLookup(c.g, 1), adjLookup(c.b, 2));
  if (uAdjKind == 2) return adjHueSaturation(c);
  if (uAdjKind == 3) return adjVibrance(c);
  if (uAdjKind == 4) return adjColorBalance(c);
  if (uAdjKind == 5) return adjBlackWhite(c);
  if (uAdjKind == 6) {
    float t = clamp(lum(c), 0.0, 1.0);
    if (uAdjP[0].x > 0.5) t = 1.0 - t;
    if (uAdjP[0].y > 0.5) t += (dissolveThreshold(px) - 0.5) / 255.0;
    return vec3(adjLookup(t, 0), adjLookup(t, 1), adjLookup(t, 2));
  }
  if (uAdjKind == 7) return vec3(lum(c) * 255.0 + 1e-3 >= uAdjP[0].x ? 1.0 : 0.0);
  return c;
}
// an adjustment layer over a premultiplied backdrop: mixed in by k, alpha unchanged
vec4 adjustPx(int mode, vec4 dst, float k, ivec2 px) {
  float a = dst.a;
  if (a <= 0.0) return dst;
  if (mode == 1) k = dissolveThreshold(px) < k ? 1.0 : 0.0;
  if (k <= 0.0) return dst;
  vec3 cb = clamp(dst.rgb / a, 0.0, 1.0);
  vec3 adj = applyAdjust(cb, px);
  vec3 b = (mode == 0 || mode == 1) ? adj : blendRGB(mode, cb, adj);
  return vec4((cb + (b - cb) * k) * a, a);
}
`;
