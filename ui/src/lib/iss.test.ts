import { describe, expect, it } from 'vitest';
import { ageDays, footprintDeg, freshness, freshnessNote, groundAt, mapView, overhead, slack, until, upcoming } from './iss';

const pass = (rise: number, set: number) => ({ rise, top: (rise + set) / 2, set, max_elevation: 30 });
const passes = [pass(1000, 1500), pass(7000, 7400)];

describe('orbit data age', () => {
  it('grades and explains it', () => {
    expect(ageDays(null, 0)).toBeNull();
    expect(freshness(null)).toBe('none');
    expect(freshness(1)).toBe('fresh');
    expect(freshness(5)).toBe('aging');
    expect(freshness(20)).toBe('stale');
    expect(freshnessNote(1)).toBe('');
    expect(freshnessNote(5.7)).toContain('5 days old');
    expect(freshnessNote(20)).toContain('go online');
  });

  it('widens passes as it ages, up to 10 min', () => {
    expect(slack(1)).toBe(0);
    expect(slack(7)).toBe(100);
    expect(slack(400)).toBe(600);
  });
});

describe('overhead', () => {
  it('finds the pass under way', () => {
    expect(overhead(passes, 1200, 1)).toBe(passes[0]);
    expect(overhead(passes, 1600, 1)).toBeNull();
    expect(overhead(passes, 7000, 1)).toBe(passes[1]);
  });

  it('counts the slack around a pass when the data is old', () => {
    expect(overhead(passes, 1550, 7)).toBe(passes[0]);
    expect(overhead(passes, 950, 7)).toBe(passes[0]);
    expect(overhead(passes, 1700, 7)).toBeNull();
  });

  it('lists passes not yet ended', () => {
    expect(upcoming(passes, 1200)).toEqual(passes);
    expect(upcoming(passes, 2000)).toEqual([passes[1]]);
  });

  it('says how long until', () => {
    expect(until(360, 0)).toBe('in 6 min');
    expect(until(7500, 0)).toBe('in 2 h 5 min');
  });
});

describe('map', () => {
  const pt = (t: number, lat: number, lon: number) => ({ t, lat, lon, alt_km: 420 });

  it('interpolates the ground point, across the date line too', () => {
    const tr = [pt(0, 0, 170), pt(30, 2, -170), pt(60, 4, -150)];
    expect(groundAt(tr, 15)).toMatchObject({ lat: 1, lon: -180 });
    expect(groundAt(tr, 45)).toMatchObject({ lat: 3, lon: -160 });
    expect(groundAt(tr, -1)).toBeNull();
    expect(groundAt(tr, 61)).toBeNull();
  });

  it('gives the station a footprint about 20° across in radius', () => {
    expect(footprintDeg(420)).toBeGreaterThan(19);
    expect(footprintDeg(420)).toBeLessThan(21);
  });

  it('shows the whole world unless zoomed to a known site', () => {
    expect(mapView(null, true)).toBe('0 0 360 180');
    expect(mapView({ lat: 40, lon: -105 }, false)).toBe('0 0 360 180');
    expect(mapView({ lat: 40, lon: -105 }, true)).toBe('30 27.5 90 45');
    expect(mapView({ lat: 85, lon: 179 }, true)).toBe('270 0 90 45');
  });
});
