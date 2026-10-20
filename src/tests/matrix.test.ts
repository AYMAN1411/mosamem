import { describe, expect, it } from 'vitest';
import { about, apply, convex, homography, multiply, rotate, translate } from '../engine/matrix';

describe('3×3 matrices', () => {
  it('maps the four corners exactly', () => {
    const from = [{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 50 }, { x: 0, y: 50 }];
    const to = [{ x: 10, y: 5 }, { x: 120, y: 20 }, { x: 90, y: 80 }, { x: -5, y: 60 }];
    const m = homography(from, to)!;
    from.forEach((p, k) => {
      const q = apply(m, p);
      expect(q.x).toBeCloseTo(to[k].x, 9);
      expect(q.y).toBeCloseTo(to[k].y, 9);
    });
  });

  it('rotates about a point and composes', () => {
    const m = about({ x: 10, y: 10 }, rotate(Math.PI / 2));
    const p = apply(m, { x: 20, y: 10 });
    expect(p.x).toBeCloseTo(10, 9);
    expect(p.y).toBeCloseTo(20, 9);
    const q = apply(multiply(translate(5, 0), m), { x: 20, y: 10 });
    expect(q.x).toBeCloseTo(15, 9);
  });

  it('tells convex quads from folded ones', () => {
    expect(convex([{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }, { x: 0, y: 10 }])).toBe(true);
    expect(convex([{ x: 0, y: 0 }, { x: 10, y: 10 }, { x: 10, y: 0 }, { x: 0, y: 10 }])).toBe(false);
  });
});
