import { describe, expect, it } from 'vitest';
import type { Aircraft, Flight } from './api';
import { TRAIL_KEEP_MS, adsbSummary, aircraftFacts, aircraftLine, aircraftRows, TRAIL_FADE_MS, extendTrails, heightColor, tracksToDraw, trailPieces, filterFlights, flightDays, flightLine, fmtClimb, isAlarm, mapFrame, pickedFirst, trackPath, trailPath } from './adsb';

const plane = (icao: string, o: Partial<Aircraft> = {}): Aircraft => ({
  icao,
  callsign: null,
  alt_ft: null,
  speed_kt: null,
  heading_deg: null,
  airspeed: false,
  vrate_fpm: null,
  registration: null,
  country: null,
  category: null,
  squawk: null,
  emergency: null,
  advisory: false,
  alert: false,
  spi: false,
  on_ground: null,
  gnss_alt_ft: null,
  selected_alt_ft: null,
  selected_alt_fms: false,
  selected_heading_deg: null,
  baro_hpa: null,
  modes: [],
  adsb_version: null,
  broadcasting: true,
  seen_s: 0,
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
  it('freshest position first, by the minute, then nearest', () => {
    const l = [plane('A00001', { lat: 40.5, lon: -104.99, pos_age_s: 70 }), plane('A00002', { lat: 40.5, lon: -104.99, pos_age_s: 5 }), plane('A00003'), plane('A00004', { lat: 39.8, lon: -105.0, pos_age_s: 50 })];
    expect(aircraftRows(l, home, true).map((r) => r.a.icao)).toEqual(['A00004', 'A00002', 'A00001', 'A00003']);
  });
  it('without a location, only by address', () => {
    expect(aircraftRows(list, null).map((r) => r.a.icao)).toEqual(['A00001', 'A00002', 'B00003', 'C00001']);
  });

  it('moves the picked plane to the top of the list', () => {
    const rows = aircraftRows([plane('A00001'), plane('A00002'), plane('A00003')], null);
    expect(pickedFirst(rows, 'A00003').map((r) => r.a.icao)).toEqual(['A00003', 'A00001', 'A00002']);
    expect(pickedFirst(rows, 'A00001')).toBe(rows);
    expect(pickedFirst(rows, null)).toBe(rows);
    expect(pickedFirst(rows, 'FFFFFF')).toBe(rows);
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

describe('logged flights and trails', () => {
  const plane = (o: Partial<Aircraft>): Aircraft => ({ icao: 'A835AF', callsign: null, alt_ft: 3000, speed_kt: null, heading_deg: null, airspeed: false, vrate_fpm: null, lat: 25.7, lon: -80.2, age_s: 0, pos_age_s: 0, messages: 3, snr_db: 10, registration: 'N628TS', country: 'United States', category: null, squawk: null, emergency: null, advisory: false, alert: false, spi: false, on_ground: null, gnss_alt_ft: null, selected_alt_ft: null, selected_alt_fms: false, selected_heading_deg: null, baro_hpa: null, modes: [], adsb_version: null, broadcasting: true, seen_s: 1, ...o });
  const flight = (o: Partial<Flight>): Flight => ({ icao: 'A835AF', callsign: null, registration: 'N628TS', country: 'United States', category: null, squawk: null, emergency: null, first_ms: 0, last_ms: 60_000, points: 2, max_alt_ft: 3000, messages: 0, ...o });

  it('adds points as an aircraft moves, not while it sits', () => {
    const one = extendTrails({}, [plane({})], 1000);
    expect(one.A835AF.length).toBe(1);
    expect(extendTrails(one, [plane({})], 2000)).toBe(one);
    expect(extendTrails(one, [plane({ lat: 25.8 })], 2000).A835AF.length).toBe(2);
  });
  it('skips a stale position and drops an old trail', () => {
    expect(extendTrails({}, [plane({ pos_age_s: 300 })], 1000)).toEqual({});
    const one = extendTrails({}, [plane({})], 1000);
    expect(extendTrails(one, [], 1000 + TRAIL_KEEP_MS + 1)).toEqual({});
  });
  it('lifts the pen over a long gap', () => {
    const d = trackPath([{ t: 0, la: 25.7, lo: -80.2 }, { t: 1, la: 25.71, lo: -80.2 }, { t: 2, la: 27, lo: -80.2 }], { lat: 25.7, lon: -80.2 });
    expect(d.match(/M/g)?.length).toBe(2);
    expect(d.match(/L/g)?.length).toBe(1);
  });
  it('flags emergencies, by state or by squawk', () => {
    expect(isAlarm(plane({ emergency: 'Minimum fuel' }))).toBe(true);
    expect(isAlarm(plane({ squawk: '7700' }))).toBe(true);
    expect(isAlarm(plane({ squawk: '1200' }))).toBe(false);
  });
  it('lists facts it has and only those', () => {
    const f = aircraftFacts(plane({ squawk: '1200', modes: ['LNAV'] }));
    expect(f.find((x) => x.label === 'Squawk')?.value).toBe('1200');
    expect(f.find((x) => x.label === 'Modes')?.value).toBe('LNAV');
    expect(f.find((x) => x.label === 'Emergency')).toBeUndefined();
  });
  it('groups flights by day and finds them', () => {
    const now = new Date(2026, 10, 3, 12).getTime();
    const days = flightDays([flight({ last_ms: now }), flight({ last_ms: now - 86_400_000 })], now);
    expect(days.map((d) => d.day)).toEqual(['Today', 'Yesterday']);
    expect(filterFlights([flight({ callsign: 'AAL475' })], 'aal united').length).toBe(1);
    expect(filterFlights([flight({})], 'heavy').length).toBe(0);
  });
  it('sums up a logged flight', () => {
    expect(flightLine(flight({ category: 'Heavy', messages: 1 }))).toBe('Heavy · up to 3,000 ft · 1 min · 1 message');
    expect(flightLine(flight({ points: 0, max_alt_ft: null }))).toBe('no position');
  });
  it('holds a logged track in the frame', () => {
    const f = mapFrame([], { lat: 25.7, lon: -80.2 }, [{ t: 0, la: 27, lo: -80.2 }])!;
    expect(f.halfKm).toBe(200);
  });
});

describe('trailPath', () => {
  const pts = [{ t: 0, la: 25, lo: -80 }, { t: 1, la: 25.1, lo: -80.1 }];
  it('draws what trackPath draws', () => {
    expect(trailPath(pts, { lat: 25, lon: -80 })).toBe(trackPath(pts, { lat: 25, lon: -80 }));
  });
  it('redraws when the map centre moves', () => {
    const a = trailPath(pts, { lat: 25, lon: -80 });
    expect(trailPath(pts, { lat: 26, lon: -80 })).not.toBe(a);
  });
  it('redraws a new track and keeps the old one', () => {
    const a = trailPath(pts, { lat: 25, lon: -80 });
    const longer = [...pts, { t: 2, la: 25.2, lo: -80.2 }];
    expect(trailPath(longer, { lat: 25, lon: -80 })).not.toBe(a);
    expect(trailPath(pts, { lat: 25, lon: -80 })).toBe(a);
  });
});

describe('trailPieces', () => {
  const centre = { lat: 26, lon: -80 };
  const now = 10_000_000;
  const at = (minsAgo: number, a?: number, la = 26 + minsAgo / 100) => ({ t: now - minsAgo * 60_000, la, lo: -80, ...(a !== undefined ? { a } : {}) });

  it('colours each stretch by its height', () => {
    expect(heightColor(1000)).toBe('var(--orange)');
    expect(heightColor(35000)).toBe('var(--magenta)');
    expect(heightColor(null)).toBe('var(--fg-dim)');
    const p = trailPieces([at(1, 1000), at(0.8, 1500), at(0.5, 5000)], centre, now);
    expect(p.map((x) => x.color)).toEqual(['var(--orange)', 'var(--green)']);
  });

  it('older lines are fainter and past the fade are gone', () => {
    const p = trailPieces([at(20, 1000), at(16, 1000), at(12, 1000), at(0.1, 1000)], centre, now);
    expect(p).toHaveLength(2);
    expect(p[0].opacity).toBeLessThan(p[1].opacity);
    expect(p[1].opacity).toBe(1);
    expect(trailPieces([at(TRAIL_FADE_MS / 60000 + 5, 1000), at(TRAIL_FADE_MS / 60000 + 1, 1000)], centre, now)).toEqual([]);
  });

  it('keeps every line at full strength when there is no fade', () => {
    expect(trailPieces([at(500, 1000, 26), at(400, 1000, 26.1)], centre, now, Infinity)[0].opacity).toBe(1);
  });

  it('lifts the pen over a stretch not heard', () => {
    expect(trailPieces([at(2, 1000, 26), at(1, 1000, 28)], centre, now)).toEqual([]);
  });
});

describe('tracksToDraw', () => {
  const centre = { lat: 26, lon: -80 };
  const now = 100_000_000;
  const line = (minsAgo: number) => [{ t: now - minsAgo * 60_000 - 1000, la: 26, lo: -80, a: 1000 }, { t: now - minsAgo * 60_000, la: 26.01, lo: -80, a: 1000 }];

  it('draws a line only for a plane on the map', () => {
    const d = tracksToDraw({ A00001: line(1), A00002: line(1) }, [{ icao: 'A00001' }], null, centre, now);
    expect(Object.keys(d)).toEqual(['A00001']);
  });

  it('keeps the picked plane past the fade', () => {
    const t = { A00001: line(TRAIL_FADE_MS / 60000 + 5) };
    expect(tracksToDraw(t, [{ icao: 'A00001' }], null, centre, now).A00001).toEqual([]);
    expect(tracksToDraw(t, [{ icao: 'A00001' }], 'A00001', centre, now).A00001).toHaveLength(1);
  });
});
