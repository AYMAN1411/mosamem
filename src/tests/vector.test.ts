import { describe, expect, it } from 'vitest';
import { dragShape, segments, shapePath, type Pt } from '../engine/vector';

const opts = { square: false, center: false, radius: 0, sides: 6, inner: 0.5, points: 5, arrowStart: false, arrowEnd: true };

describe('dragShape', () => {
  it('draws a rectangle from any corner with positive size', () => {
    expect(dragShape('rect', [100, 80], [40, 20], opts)).toEqual({ kind: 'rect', x: 40, y: 20, w: 60, h: 60, radius: 0 });
  });
  it('keeps it square with Shift and centered with Alt', () => {
    expect(dragShape('ellipse', [100, 100], [130, 110], { ...opts, square: true, center: true })).toEqual({ kind: 'ellipse', x: 70, y: 70, w: 60, h: 60 });
  });
  it('turns a polygon by the drag direction and snaps it with Shift', () => {
    const s = dragShape('polygon', [0, 0], [0, -50], opts);
    expect(s).toMatchObject({ kind: 'polygon', r: 50, sides: 6 });
    expect((s as { angle: number }).angle).toBeCloseTo(0);
    const t = dragShape('star', [0, 0], [10, -50], { ...opts, square: true });
    expect((t as { angle: number }).angle).toBe(15);
  });
  it('snaps a line to 45° with Shift', () => {
    const s = dragShape('line', [0, 0], [100, 90], { ...opts, square: true }) as { a: Pt; b: Pt };
    expect(s.b[0]).toBeCloseTo(s.b[1]);
  });
});

describe('shapePath', () => {
  it('puts a polygon point at the top at angle 0', () => {
    const p = shapePath({ kind: 'polygon', cx: 100, cy: 100, r: 50, sides: 6, angle: 0 });
    const k = p.subs[0].knots;
    expect(k).toHaveLength(6);
    expect(k[0].p[0]).toBeCloseTo(100);
    expect(k[0].p[1]).toBeCloseTo(50);
  });
  it('alternates outer and inner radii for a star', () => {
    const k = shapePath({ kind: 'star', cx: 0, cy: 0, r: 10, inner: 0.4, points: 5, angle: 0 }).subs[0].knots;
    expect(k).toHaveLength(10);
    expect(Math.hypot(...k[0].p)).toBeCloseTo(10);
    expect(Math.hypot(...k[1].p)).toBeCloseTo(4);
  });
  it('closes a rounded rectangle with 8 curves that meet', () => {
    const s = shapePath({ kind: 'rect', x: 0, y: 0, w: 100, h: 50, radius: 10 }).subs[0];
    const seg = segments(s);
    expect(seg).toHaveLength(8);
    seg.forEach((c, i) => expect(c[3]).toEqual(seg[(i + 1) % 8][0]));
  });
  it('clamps the radius to half the short side', () => {
    const k = shapePath({ kind: 'rect', x: 0, y: 0, w: 100, h: 20, radius: 50 }).subs[0].knots;
    expect(k[0].p).toEqual([10, 0]);
  });
});
