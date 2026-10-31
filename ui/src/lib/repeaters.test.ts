import { describe, expect, it } from 'vitest';
import type { Band, Repeater } from './api';
import { kmBetween, repeaterPoints, townLabels } from './repeaters';

const twoM: Band = { id: '2m', group: 'ham', name: '2 m', icon: 'tower', mode: 'nfm', low_mhz: 144, high_mhz: 148, start_mhz: 146.52, bookmarks: [] };
const rep = (call: string, city: string, mhz: number, lat: number, lon: number, modes: string[] = []): Repeater => ({ call, city, mhz, lat, lon, shift: '-', tone: '100.0', modes });
const home = { lat: 40.0, lon: -105.0 };

describe('repeater map', () => {
  const list = [
    rep('FAR', 'Greeley', 147.3, 40.42, -104.71),
    rep('NEAR', 'Northglenn', 147.045, 39.886, -104.987),
    rep('DIGI', 'Boulder', 145.25, 40.015, -105.27, ['DMR', 'Fusion']),
    rep('UHF', 'Denver', 447.0, 39.74, -104.99),
  ];
  it('keeps the band, nearest first, and marks reach and maybe-digital', () => {
    const p = repeaterPoints(list, twoM, home);
    expect(p.map((r) => r.call)).toEqual(['NEAR', 'DIGI', 'FAR']);
    expect(p[0].km).toBeCloseTo(12.7, 0);
    expect(p.map((r) => r.reach)).toEqual([true, true, false]);
    expect(p[1].digital).toBe('DMR');
    expect(p[0].digital).toBeNull();
  });
  it('places east as +x and south as +y', () => {
    const [near, digi] = repeaterPoints(list, twoM, home);
    expect(near.y).toBeGreaterThan(0);
    expect(digi.x).toBeLessThan(0);
    expect(Math.hypot(near.x, near.y)).toBeCloseTo(near.km, 0);
  });
  it('treats repeaters past the band reach as out of reach', () => {
    const p = repeaterPoints([rep('SKI', 'Steamboat', 146.7, 40.48, -106.83)], twoM, home);
    expect(p[0].reach).toBe(false);
  });
  it('labels the towns with the most repeaters', () => {
    const p = repeaterPoints([...list, rep('NEAR2', 'Northglenn', 146.7, 39.9, -104.98)], twoM, home);
    expect(townLabels(p, 1).map((t) => t.name)).toEqual(['Northglenn']);
  });
  it('measures distance on the sphere', () => {
    expect(kmBetween({ lat: 39.74, lon: -104.99 }, { lat: 40.015, lon: -105.27 })).toBeCloseTo(38.6, 0);
  });
});
