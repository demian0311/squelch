// Space station passes: which one is under way, and how far to trust the orbit data.
// Times are Unix seconds, as the Rust side sends them.
import type { GroundPoint, IssPass } from './api';

const DAY = 86_400;

/** Orbit data age in days, or null with none yet. */
export function ageDays(tleEpoch: number | null, now: number): number | null {
  return tleEpoch === null ? null : (now - tleEpoch) / DAY;
}

/** Seconds of slack around each pass. Drag and reboosts move the station off an old prediction. */
export function slack(age: number | null): number {
  return age === null ? 0 : Math.min(600, Math.max(0, age - 2) * 20);
}

export type Freshness = 'none' | 'fresh' | 'aging' | 'stale';

export function freshness(age: number | null): Freshness {
  if (age === null) return 'none';
  return age <= 3 ? 'fresh' : age <= 10 ? 'aging' : 'stale';
}

/** What to say about the orbit data; empty while it is fresh. */
export function freshnessNote(age: number | null): string {
  const days = age === null ? 0 : Math.floor(age);
  switch (freshness(age)) {
    case 'none':
      return 'No orbit data yet. It downloads when the computer is online.';
    case 'aging':
      return `Orbit data is ${days} days old. Times may be off by a minute or two.`;
    case 'stale':
      return `Orbit data is ${days} days old. Times may be off by several minutes; go online to refresh it.`;
    default:
      return '';
  }
}

/** How old the orbit data is, for a label that is always shown. */
export function ageLabel(age: number | null): string {
  if (age === null) return 'No orbit data';
  const h = Math.floor(age * 24);
  if (h < 1) return 'Orbit data under an hour old';
  if (h < 48) return `Orbit data ${h} hour${h === 1 ? '' : 's'} old`;
  return `Orbit data ${Math.floor(age)} days old`;
}

/** The pass the station may be in now, widened by `slack` for old data. */
export function overhead(passes: IssPass[], now: number, age: number | null): IssPass | null {
  const s = slack(age);
  return passes.find((p) => p.rise - s <= now && now <= p.set + s) ?? null;
}

/** Passes that have not ended yet. */
export function upcoming(passes: IssPass[], now: number): IssPass[] {
  return passes.filter((p) => p.set > now);
}

/** "Today", "Tomorrow" or a weekday, in local time. */
export function dayLabel(t: number, now: number): string {
  const day = (x: number) => new Date(x * 1000).setHours(0, 0, 0, 0);
  const d = Math.round((day(t) - day(now)) / (DAY * 1000));
  if (d === 0) return 'Today';
  if (d === 1) return 'Tomorrow';
  return new Date(t * 1000).toLocaleDateString([], { weekday: 'long' });
}

export function clock(t: number): string {
  return new Date(t * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

/** "in 3 min", "in 2 h 5 min". */
export function until(t: number, now: number): string {
  const m = Math.max(0, Math.round((t - now) / 60));
  return m < 60 ? `in ${m} min` : `in ${Math.floor(m / 60)} h ${m % 60} min`;
}

/** Rail tooltip for the station's band. */
export function issTitle(passes: IssPass[], now: number, age: number | null): string {
  const o = overhead(passes, now, age);
  if (o) return now < o.rise || now > o.set ? 'Space station possibly overhead now' : `Space station overhead now · ${Math.round(o.max_elevation)}° high`;
  const next = upcoming(passes, now)[0];
  return next ? `Space station · next pass ${dayLabel(next.rise, now).toLowerCase()} ${clock(next.rise)}` : 'Space station';
}

const EARTH_KM = 6371;

/** Where the station is at `t`, interpolated along the track; null outside it. */
export function groundAt(track: GroundPoint[], t: number): GroundPoint | null {
  const i = track.findIndex((p) => p.t > t);
  if (i <= 0) return null;
  const [a, b] = [track[i - 1], track[i]];
  const f = (t - a.t) / (b.t - a.t);
  let dLon = b.lon - a.lon;
  if (dLon > 180) dLon -= 360;
  if (dLon < -180) dLon += 360;
  const lon = a.lon + f * dLon;
  return { t, lat: a.lat + f * (b.lat - a.lat), lon: ((lon + 540) % 360) - 180, alt_km: a.alt_km + f * (b.alt_km - a.alt_km) };
}

/** Radius in degrees of arc of the ground the station can be heard from: where it is above the horizon. */
export function footprintDeg(altKm: number): number {
  return (Math.acos(EARTH_KM / (EARTH_KM + altKm)) * 180) / Math.PI;
}

/** dB above the floor that counts as hearing something on the station's channel. Empty channels read 3–10 dB. */
export const RECEPTION_DB = 12;

/**
 * Map area as an SVG viewBox over an equirectangular world 360 × 180 (x = lon + 180, y = 90 − lat).
 * Zoomed to the listener only while a pass is under way and something is heard; the whole world otherwise.
 */
export function mapView(site: { lat: number; lon: number } | null, local: boolean): string {
  if (!site || !local) return '0 0 360 180';
  const [w, h] = [90, 45];
  const x = Math.min(360 - w, Math.max(0, site.lon + 180 - w / 2));
  const y = Math.min(180 - h, Math.max(0, 90 - site.lat - h / 2));
  return `${x} ${y} ${w} ${h}`;
}
