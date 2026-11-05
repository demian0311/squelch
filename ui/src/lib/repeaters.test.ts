import { describe, expect, it } from 'vitest';
import type { Band, Repeater } from './api';
import { clampView, heldOutputs, hueOf, kmBetween, liveOutputs, mapMarks, NEAREST_KM, readView, repeaterPoints, ringReach, townLabels, WIDEST_KM, zoomView, type RepeaterPoint } from './repeaters';

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
  it('leaves out a signal the squelch would mute', () => {
    const signals = [{ hz: 146_760_000, snr_db: 10 }, { hz: 146_767_500, snr_db: 22 }];
    expect([...liveOutputs(points, signals, 12)]).toEqual([[146_767_500, 22]]);
    expect([...liveOutputs(points, signals, 10)]).toEqual([[146_760_000, 10], [146_767_500, 22]]);
    expect(liveOutputs(points, signals, 0).size).toBe(2);
  });
});

describe('heldOutputs', () => {
  it('keeps a repeater on air through a pause, then lets it go', () => {
    let held = heldOutputs(new Map(), new Map([[146_760_000, 20]]), 1_000);
    held = heldOutputs(held, new Map(), 3_900);
    expect(held.get(146_760_000)).toEqual({ snr: 20, at: 1_000 });
    expect(heldOutputs(held, new Map(), 4_000).size).toBe(0);
  });
  it('a reading rises at once and falls slowly, and restarts the hold', () => {
    const was = new Map([[146_760_000, { snr: 20, at: 1_000 }]]);
    expect(heldOutputs(was, new Map([[146_760_000, 9]]), 2_000).get(146_760_000)).toEqual({ snr: 17, at: 2_000 });
    expect(heldOutputs(was, new Map([[146_760_000, 30]]), 2_000).get(146_760_000)).toEqual({ snr: 30, at: 2_000 });
  });
});

describe('ringReach', () => {
  it('goes up in steps with the signal', () => {
    expect([8, 15, 24, 25, 35, 60].map(ringReach)).toEqual([0.5, 0.75, 0.75, 1, 1.3, 1.3]);
  });
});

describe('map view', () => {
  it('never goes closer, wider, or further off than the widest view', () => {
    expect(clampView({ x: 500, y: -500, halfKm: 1000 })).toEqual({ x: WIDEST_KM, y: -WIDEST_KM, halfKm: WIDEST_KM });
    expect(clampView({ x: 0, y: 0, halfKm: 0.1 }).halfKm).toBe(NEAREST_KM);
  });
  it('zooms around the place under the pointer, which stays put', () => {
    const v = { x: 0, y: 0, halfKm: 40 };
    const at = { x: 10, y: -20 };
    const z = zoomView(v, 0.5, at);
    expect(z.halfKm).toBe(20);
    // the pointer's place was 10 km right of centre in a 40 km view: still a quarter of the view right of the new centre
    expect((at.x - z.x) / z.halfKm).toBeCloseTo((at.x - v.x) / v.halfKm);
    expect((at.y - z.y) / z.halfKm).toBeCloseTo((at.y - v.y) / v.halfKm);
  });
  it('reads a saved view and falls back on nonsense', () => {
    expect(readView({ x: 1, y: 2, halfKm: 30 })).toEqual({ x: 1, y: 2, halfKm: 30 });
    expect(readView(75)).toEqual({ x: 0, y: 0, halfKm: WIDEST_KM });
    expect(readView({ x: 'a' })).toEqual({ x: 0, y: 0, halfKm: WIDEST_KM });
  });
});

describe('marks', () => {
  const pt = (id: string, x: number, y: number, more: Partial<RepeaterPoint> = {}): RepeaterPoint => ({ id, hz: 146e6, call: id, city: 'Miami', x, y, km: Math.hypot(x, y), reach: true, digital: null, ...more });
  it('says green for reach, blue for maybe digital, red for too far', () => {
    expect(hueOf({ reach: true, digital: null })).toBe('green');
    expect(hueOf({ reach: true, digital: 'DMR' })).toBe('blue');
    expect(hueOf({ reach: false, digital: null })).toBe('red');
  });
  it('folds neighbours on screen into one badge around the nearest', () => {
    const marks = mapMarks([pt('a', 0, 0), pt('b', 1, 0), pt('c', 30, 0, { reach: false })], 0.1, false);
    expect(marks.map((m) => m.kind)).toEqual(['many', 'one']);
    const badge = marks[0] as Extract<(typeof marks)[number], { kind: 'many' }>;
    expect(badge.points.map((p) => p.id)).toEqual(['a', 'b']);
    expect(badge.x).toBe(0.5);
    expect(badge.hue).toBe('green');
  });
  it('a badge is blue only when none of it is plain FM in reach, red when none is in reach', () => {
    const blue = mapMarks([pt('a', 0, 0, { digital: 'DMR' }), pt('b', 1, 0, { reach: false })], 0.1, false)[0];
    expect(blue.kind === 'many' && blue.hue).toBe('blue');
    const red = mapMarks([pt('a', 0, 0, { reach: false }), pt('b', 1, 0, { reach: false })], 0.1, false)[0];
    expect(red.kind === 'many' && red.hue).toBe('red');
  });
  it('the tuned repeater and those on air never fold into a badge', () => {
    const marks = mapMarks([pt('a', 0, 0), pt('b', 1, 0), pt('c', 1, 1)], 0.1, false, new Set(['b']));
    expect(marks.map((m) => (m.kind === 'one' ? m.p.id : m.points.map((p) => p.id).join('+')))).toEqual(['a+c', 'b']);
  });
  it('a badge on the tuned tower moves beside it, on a line back to the tower', () => {
    const marks = mapMarks([pt('t', 0, 0), pt('a', 0, 0), pt('b', 0.1, 0)], 0.1, false, new Set(['t']));
    const badge = marks.find((m) => m.kind === 'many') as Extract<(typeof marks)[number], { kind: 'many' }>;
    expect(badge.from).toEqual({ x: 0.05, y: 0 });
    expect(badge.x).toBeGreaterThan(2);
    expect(badge.y).toBe(0);
  });
  it('at the nearest zoom nothing folds, and a shared tower fans out', () => {
    const marks = mapMarks([pt('a', 0, 0), pt('b', 0, 0), pt('c', 0.5, 0)], 0.01, true);
    expect(marks.every((m) => m.kind === 'one')).toBe(true);
    const fanned = marks.filter((m) => m.kind === 'one' && m.from);
    expect(fanned.map((m) => (m as { p: RepeaterPoint }).p.id)).toEqual(['a', 'b']);
    expect(fanned.map((m) => m.x)).not.toEqual([0, 0]);
  });
});
