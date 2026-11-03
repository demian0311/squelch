// Repeaters on the map: where each sits from the listener, and whether it is likely in reach.
import type { Band, Repeater } from './api';

/**
 * Typical reach per band, in km. The RepeaterBook export has no power or antenna height,
 * so one figure per band stands in; a mountaintop site reaches much further.
 */
export const REACH_KM: Record<string, number> = { '6m': 60, '2m': 50, '125': 40, '70cm': 30 };

// squelch plays analog FM only; a repeater also listed in one of these may be digital only
const DIGITAL = ['DMR', 'P25', 'NXDN', 'D-STAR', 'M17'];

/** How far the map can show around the listener, in km: the slider's stops. */
export const RANGES_KM = [25, 50, 75, 100, 150, 200, 300];
export const DEFAULT_RANGE_KM = 75;

// a signal this close to a repeater's output is taken to be that repeater
const ON_AIR_HZ = 5_000;

// how long a repeater still counts as on air after its signal stops
const HOLD_MS = 3_000;
// a reading rises at once and falls this fast, so the circles don't jump while someone talks
const FALL_DB_PER_SEC = 3;
// how far the on-air circles travel, by dB above the noise: [below this dB, share of their full size]
const REACH_STEPS = [[15, 0.5], [25, 0.75], [35, 1]];
const STRONGEST_REACH = 1.3;

export interface HeardOutput {
  snr: number;
  at: number;
}

export interface RepeaterPoint {
  id: string;
  hz: number;
  call: string;
  city: string;
  /** km east and south of the listener, so +y runs down the screen */
  x: number;
  y: number;
  km: number;
  reach: boolean;
  /** the digital mode it is also listed under, when it may be digital only */
  digital: string | null;
}

const EARTH_KM = 6371;
const rad = (d: number) => (d * Math.PI) / 180;

/** Great-circle distance in km. */
export function kmBetween(a: { lat: number; lon: number }, b: { lat: number; lon: number }): number {
  const h = Math.sin(rad(b.lat - a.lat) / 2) ** 2 + Math.cos(rad(a.lat)) * Math.cos(rad(b.lat)) * Math.sin(rad(b.lon - a.lon) / 2) ** 2;
  return 2 * EARTH_KM * Math.asin(Math.sqrt(h));
}

/** The band's repeaters placed around `site`, nearest first. */
export function repeaterPoints(list: Repeater[], band: Band, site: { lat: number; lon: number }): RepeaterPoint[] {
  const reachKm = REACH_KM[band.id] ?? 0;
  const kmPerLon = (EARTH_KM * Math.PI * Math.cos(rad(site.lat))) / 180;
  const kmPerLat = (EARTH_KM * Math.PI) / 180;
  return list
    .filter((r) => r.mhz >= band.low_mhz && r.mhz <= band.high_mhz)
    .map((r) => {
      const km = kmBetween(site, r);
      return {
        id: r.id,
        hz: Math.round(r.mhz * 1e6),
        call: r.call,
        city: r.city,
        x: (r.lon - site.lon) * kmPerLon,
        y: (site.lat - r.lat) * kmPerLat,
        km,
        reach: km <= reachKm,
        digital: r.modes.find((m) => DIGITAL.includes(m)) ?? null,
      };
    })
    .sort((a, b) => a.km - b.km);
}

/** Output frequencies with a signal on them now, with the strongest signal's dB above the noise. */
export function liveOutputs(points: RepeaterPoint[], signals: { hz: number; snr_db: number }[]): Map<number, number> {
  const live = new Map<number, number>();
  for (const s of signals) {
    let best: number | null = null;
    for (const p of points) if (Math.abs(p.hz - s.hz) <= ON_AIR_HZ && (best === null || Math.abs(p.hz - s.hz) < Math.abs(best - s.hz))) best = p.hz;
    if (best !== null) live.set(best, Math.max(live.get(best) ?? -Infinity, s.snr_db));
  }
  return live;
}

/** Output frequencies heard lately: a pause between words or a dropped detection must not end the rings. */
export function heldOutputs(prev: Map<number, HeardOutput>, live: Map<number, number>, now: number): Map<number, HeardOutput> {
  const held = new Map([...prev].filter(([, h]) => now - h.at < HOLD_MS));
  for (const [hz, snr] of live) {
    const was = prev.get(hz);
    held.set(hz, { snr: was ? Math.max(snr, was.snr - (FALL_DB_PER_SEC * (now - was.at)) / 1000) : snr, at: now });
  }
  return held;
}

/** How far a repeater's on-air circles travel for a signal this many dB above the noise. */
export function ringReach(snr: number): number {
  return REACH_STEPS.find(([db]) => snr < db)?.[1] ?? STRONGEST_REACH;
}

/** Town names to print on the map: those with the most repeaters, at the middle of their repeaters. */
export function townLabels(points: RepeaterPoint[], max: number): { name: string; x: number; y: number }[] {
  const towns = new Map<string, { n: number; x: number; y: number }>();
  for (const p of points) {
    const t = towns.get(p.city) ?? { n: 0, x: 0, y: 0 };
    towns.set(p.city, { n: t.n + 1, x: t.x + p.x, y: t.y + p.y });
  }
  return [...towns]
    .sort((a, b) => b[1].n - a[1].n)
    .slice(0, max)
    .map(([name, t]) => ({ name, x: t.x / t.n, y: t.y / t.n }));
}
