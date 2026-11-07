import { describe, expect, it } from 'vitest';
import { clampView, ringKm, viewOf, zoomView } from './mapview';

const lim = { nearestKm: 1.5, widestKm: 400 };

describe('map view', () => {
  it('clamps to the limits and keeps the edge inside the widest view', () => {
    expect(clampView({ x: 900, y: -900, halfKm: 1000 }, lim)).toEqual({ x: 0, y: 0, halfKm: 400 });
    expect(clampView({ x: 0, y: 0, halfKm: 0.1 }, lim).halfKm).toBe(1.5);
    expect(clampView({ x: 900, y: -900, halfKm: 100 }, lim)).toEqual({ x: 300, y: -300, halfKm: 100 });
  });
  it('zooms around a point so it stays put on screen', () => {
    const v = viewOf(100);
    const at = { x: 40, y: -20 };
    const z = zoomView(v, 0.5, lim, at);
    expect(z.halfKm).toBe(50);
    // the point's offset from the centre, in half-heights, is unchanged
    expect((at.x - z.x) / z.halfKm).toBeCloseTo((at.x - v.x) / v.halfKm);
    expect((at.y - z.y) / z.halfKm).toBeCloseTo((at.y - v.y) / v.halfKm);
  });
  it('picks the widest ring that fits', () => {
    expect([50, 100, 400, 1.5, 36].map((h) => ringKm(h))).toEqual([50, 100, 400, 1, 25]);
    expect(ringKm(75, 0.9)).toBe(50);
  });
});
