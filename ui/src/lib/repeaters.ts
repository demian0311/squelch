// Repeaters on the map: where each sits from the listener, and whether it is likely in reach.
import type { Band, Repeater } from './api';

/**
 * Typical reach per band, in km. The RepeaterBook export has no power or antenna height,
 * so one figure per band stands in; a mountaintop site reaches much further.
 */
export const REACH_KM: Record<string, number> = { '2m': 50, '70cm': 30 };

// squelch plays analog FM only; a repeater also listed in one of these may be digital only
const DIGITAL = ['DMR', 'P25', 'NXDN', 'D-STAR', 'M17'];

export interface RepeaterPoint {
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
