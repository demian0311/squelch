// Aircraft heard on 1090 MHz: list rows, labels and the map's frame.
import type { Adsb, Aircraft } from './api';
import { kmBetween } from './repeaters';
import { toKm, type Site } from './terrain';

/** A position older than this is drawn faded: the aircraft has moved on since. */
export const STALE_POS_S = 30;
/** Map half-heights to choose from, km: the smallest that holds every aircraft. */
const MAP_STEPS_KM = [50, 100, 200, 400];

export interface AircraftRow {
  a: Aircraft;
  name: string;
  /** km from the listener, when both positions are known. */
  km: number | null;
}

/** Nearest first when the listener's location is set, then the rest by address so rows stay put. */
export function aircraftRows(list: Aircraft[], site: Site | null): AircraftRow[] {
  return list
    .map((a) => ({ a, name: aircraftName(a), km: site && a.lat !== null && a.lon !== null ? kmBetween(site, { lat: a.lat, lon: a.lon }) : null }))
    .sort((x, y) => (x.km ?? Infinity) - (y.km ?? Infinity) || x.a.icao.localeCompare(y.a.icao));
}

export const aircraftName = (a: Aircraft) => a.callsign ?? a.icao;

export function fmtAlt(ft: number | null): string {
  return ft === null ? '' : `${Math.round(ft).toLocaleString('en-US')} ft`;
}

export function fmtClimb(fpm: number | null): string {
  if (fpm === null || Math.abs(fpm) < 200) return '';
  return `${fpm > 0 ? '↑' : '↓'} ${Math.abs(fpm).toLocaleString('en-US')} ft/min`;
}

/** Altitude, speed and climb in one line. */
export function aircraftLine(a: Aircraft): string {
  return [fmtAlt(a.alt_ft), a.speed_kt !== null ? `${Math.round(a.speed_kt)} kt` : '', fmtClimb(a.vrate_fpm)].filter(Boolean).join(' · ');
}

export function adsbSummary(info: Adsb | null): string {
  if (!info || !info.aircraft.length) return 'None heard yet';
  const placed = info.aircraft.filter((a) => a.lat !== null).length;
  const n = info.aircraft.length;
  return `${n} aircraft · ${placed} with position · ${info.frames_per_sec.toFixed(0)} messages/s`;
}

export interface MapPlane {
  icao: string;
  name: string;
  /** km east and south of the map centre */
  x: number;
  y: number;
  heading: number | null;
  stale: boolean;
}

/**
 * Where the map is centred and how far it reaches: on the listener when the location is set,
 * else on the aircraft (rounded, so the map holds still as they move).
 */
export function mapFrame(list: Aircraft[], site: Site | null): { centre: Site; halfKm: number; planes: MapPlane[] } | null {
  const placed = list.filter((a): a is Aircraft & { lat: number; lon: number } => a.lat !== null && a.lon !== null);
  if (!site && !placed.length) return null;
  const centre = site ?? {
    lat: Math.round((placed.reduce((s, a) => s + a.lat, 0) / placed.length) * 2) / 2,
    lon: Math.round((placed.reduce((s, a) => s + a.lon, 0) / placed.length) * 2) / 2,
  };
  const planes = placed.map((a) => {
    const [x, y] = toKm(centre, a.lon, a.lat);
    return { icao: a.icao, name: aircraftName(a), x, y, heading: a.heading_deg, stale: (a.pos_age_s ?? Infinity) > STALE_POS_S };
  });
  const far = Math.max(0, ...planes.map((p) => Math.max(Math.abs(p.x), Math.abs(p.y)) * 1.1));
  const halfKm = MAP_STEPS_KM.find((k) => k >= far) ?? MAP_STEPS_KM[MAP_STEPS_KM.length - 1];
  return { centre, halfKm, planes };
}
