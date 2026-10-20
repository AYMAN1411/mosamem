import { describe, expect, it } from 'vitest';
import { monotoneSpline } from '../engine/spline';

describe('monotone spline (same curve as the engine)', () => {
  it('is the identity for the default curve', () => {
    const f = monotoneSpline([[0, 0], [255, 255]]);
    for (let x = 0; x <= 255; x += 17) expect(f(x)).toBeCloseTo(x, 4);
  });

  it('passes through its points and never goes back down', () => {
    const pts: [number, number][] = [[0, 0], [64, 40], [128, 200], [255, 255]];
    const f = monotoneSpline(pts);
    for (const [x, y] of pts) expect(f(x)).toBeCloseTo(y, 3);
    let prev = -1;
    for (let x = 0; x <= 255; x++) {
      expect(f(x)).toBeGreaterThanOrEqual(prev - 1e-9);
      prev = f(x);
    }
  });

  it('is flat beyond the end points and accepts unsorted input', () => {
    const f = monotoneSpline([[200, 220], [50, 30]]);
    expect(f(10)).toBeCloseTo(30, 6);
    expect(f(250)).toBeCloseTo(220, 6);
  });
});
