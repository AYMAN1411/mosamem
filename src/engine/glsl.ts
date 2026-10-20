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
