import { describe, expect, it } from 'vitest';
import type { Band, Repeater } from './api';
import { heldOutputs, kmBetween, liveOutputs, repeaterPoints, townLabels } from './repeaters';

const twoM: Band = { id: '2m', group: 'ham', name: '2 m', icon: 'tower', mode: 'nfm', low_mhz: 144, high_mhz: 148, start_mhz: 146.52, bookmarks: [] };
const rep = (call: string, city: string, mhz: number, lat: number, lon: number, modes: string[] = []): Repeater => ({ id: `${call}-${mhz}`, call, city, mhz, lat, lon, shift: '-', tone: '100.0', modes });
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

describe('liveOutputs', () => {
  const points = repeaterPoints([rep('A', 'Boulder', 146.76, 40.0, -105.2), rep('B', 'Greeley', 146.76, 40.4, -104.7), rep('C', 'Denver', 146.7675, 39.7, -105.0)], twoM, home);
  it('gives a signal to the closest output, keeping the strongest', () => {
    const live = liveOutputs(points, [{ hz: 146_761_000, snr_db: 10 }, { hz: 146_759_500, snr_db: 22 }, { hz: 146_767_000, snr_db: 9 }]);
    expect([...live]).toEqual([[146_760_000, 22], [146_767_500, 9]]);
  });
  it('ignores a signal away from any output', () => {
    expect(liveOutputs(points, [{ hz: 146_900_000, snr_db: 30 }]).size).toBe(0);
  });
});

describe('heldOutputs', () => {
  it('keeps a repeater on air through a pause, then lets it go', () => {
    let held = heldOutputs(new Map(), new Map([[146_760_000, 20]]), 1_000);
    held = heldOutputs(held, new Map(), 3_900);
    expect(held.get(146_760_000)).toEqual({ snr: 20, at: 1_000 });
    expect(heldOutputs(held, new Map(), 4_000).size).toBe(0);
  });
  it('a new signal restarts the hold with its level', () => {
    const held = heldOutputs(new Map([[146_760_000, { snr: 20, at: 1_000 }]]), new Map([[146_760_000, 9]]), 2_000);
    expect(held.get(146_760_000)).toEqual({ snr: 9, at: 2_000 });
  });
});
