import { describe, expect, it } from 'vitest';
import type { Aircraft } from './api';
import { adsbSummary, aircraftLine, aircraftRows, fmtClimb, mapFrame } from './adsb';

const plane = (icao: string, o: Partial<Aircraft> = {}): Aircraft => ({
  icao,
  callsign: null,
  alt_ft: null,
  speed_kt: null,
  heading_deg: null,
  airspeed: false,
  vrate_fpm: null,
  lat: null,
  lon: null,
  age_s: 1,
  pos_age_s: null,
  messages: 3,
  snr_db: 15,
  ...o,
});
const home = { lat: 39.74, lon: -104.99 };

describe('aircraft list', () => {
  const list = [
    plane('C00001'),
    plane('A00002', { callsign: 'UAL123', lat: 40.5, lon: -104.99, pos_age_s: 2 }),
    plane('B00003', { lat: 39.8, lon: -105.0, pos_age_s: 2 }),
    plane('A00001'),
  ];
  it('puts the nearest first and keeps the rest by address', () => {
    const rows = aircraftRows(list, home);
    expect(rows.map((r) => r.name)).toEqual(['B00003', 'UAL123', 'A00001', 'C00001']);
    expect(rows[1].km).toBeCloseTo(84.5, 0);
    expect(rows[2].km).toBeNull();
  });
  it('without a location, only by address', () => {
    expect(aircraftRows(list, null).map((r) => r.a.icao)).toEqual(['A00001', 'A00002', 'B00003', 'C00001']);
  });
  it('says altitude, speed and climb', () => {
    expect(aircraftLine(plane('X', { alt_ft: 38000, speed_kt: 451.6, vrate_fpm: -1216 }))).toBe('38,000 ft · 452 kt · ↓ 1,216 ft/min');
    expect(fmtClimb(64)).toBe('');
    expect(aircraftLine(plane('X'))).toBe('');
  });
  it('summarises', () => {
    expect(adsbSummary(null)).toBe('None heard yet');
    expect(adsbSummary({ aircraft: list, frames_per_sec: 12.3 })).toBe('4 aircraft · 2 with position · 12 messages/s');
  });
});

describe('aircraft map', () => {
  it('centres on the listener and grows to hold the farthest', () => {
    const f = mapFrame([plane('A', { lat: 40.5, lon: -104.99, pos_age_s: 40, heading_deg: 90 })], home)!;
    expect(f.centre).toEqual(home);
    expect(f.halfKm).toBe(100);
    expect(f.planes[0].y).toBeLessThan(-80);
    expect(f.planes[0].stale).toBe(true);
  });
  it('without a location, centres on the aircraft, rounded', () => {
    const f = mapFrame([plane('A', { lat: 52.26, lon: 3.92, pos_age_s: 1 })], null)!;
    expect(f.centre).toEqual({ lat: 52.5, lon: 4 });
    expect(f.halfKm).toBe(50);
    expect(mapFrame([plane('B')], null)).toBeNull();
  });
});
