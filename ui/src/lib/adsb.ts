// Aircraft heard on 1090 MHz: list rows, labels and the map's frame.
export const ADSB_CENTRE_HZ = 1_090_000_000;
import type { Adsb, Aircraft, Flight, TrackPoint } from './api';
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
const CATEGORY_WORDS: Record<string, string> = { 'High vortex': 'Large', 'High performance': 'Fast' };
const PLANE_SIZES = /^(Light|Medium|Large|Heavy|Fast)$/;

/** The broadcast's category in everyday words. A "high vortex" plane is one in the large class. */
export function categoryText(category: string | null | undefined): string {
  return category ? CATEGORY_WORDS[category] ?? category : '';
}

export function planeKind(category: string | null | undefined): string {
  const text = categoryText(category);
  return PLANE_SIZES.test(text) ? `${text} plane` : text;
}

export function aircraftRows(list: Aircraft[], site: Site | null, freshFirst = false): AircraftRow[] {
  // by the minute, so an order by freshness does not reshuffle with every status
  const minute = (a: Aircraft) => (freshFirst ? Math.floor((a.pos_age_s ?? Infinity) / 60) : 0);
  return list
    .map((a) => ({ a, name: aircraftName(a), km: site && a.lat !== null && a.lon !== null ? kmBetween(site, { lat: a.lat, lon: a.lon }) : null }))
    .sort((x, y) => minute(x.a) - minute(y.a) || (x.km ?? Infinity) - (y.km ?? Infinity) || x.a.icao.localeCompare(y.a.icao));
}

/** The picked row on top, so its details stay in view however the distances change. */
export function pickedFirst(rows: AircraftRow[], icao: string | null): AircraftRow[] {
  const i = rows.findIndex((r) => r.a.icao === icao);
  return i < 1 ? rows : [rows[i], ...rows.slice(0, i), ...rows.slice(i + 1)];
}

export const aircraftName = (a: Aircraft) => a.callsign ?? a.registration ?? a.icao;

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
  /** Emergency, advisory or transponder alert. */
  alarm: boolean;
  alt_ft?: number | null;
}

/**
 * Where the map is centred and how far it reaches: on the listener when the location is set,
 * else on the aircraft (rounded, so the map holds still as they move).
 */
export function mapFrame(list: Aircraft[], site: Site | null, extra: TrackPoint[] = []): { centre: Site; halfKm: number; planes: MapPlane[] } | null {
  const placed = list.filter((a): a is Aircraft & { lat: number; lon: number } => a.lat !== null && a.lon !== null);
  if (!site && !placed.length && !extra.length) return null;
  const spots = [...placed.map((a) => ({ lat: a.lat, lon: a.lon })), ...extra.map((p) => ({ lat: p.la, lon: p.lo }))];
  const centre = site ?? {
    lat: Math.round((spots.reduce((s, a) => s + a.lat, 0) / spots.length) * 2) / 2,
    lon: Math.round((spots.reduce((s, a) => s + a.lon, 0) / spots.length) * 2) / 2,
  };
  const planes = placed.map((a) => {
    const [x, y] = toKm(centre, a.lon, a.lat);
    return { icao: a.icao, name: aircraftName(a), x, y, heading: a.heading_deg, stale: (a.pos_age_s ?? Infinity) > STALE_POS_S, alarm: isAlarm(a), alt_ft: a.alt_ft };
  });
  const far = Math.max(0, ...planes.map((p) => Math.max(Math.abs(p.x), Math.abs(p.y)) * 1.1), ...extra.map((p) => {
    const [x, y] = toKm(centre, p.lo, p.la);
    return Math.max(Math.abs(x), Math.abs(y)) * 1.1;
  }));
  const halfKm = MAP_STEPS_KM.find((k) => k >= far) ?? MAP_STEPS_KM[MAP_STEPS_KM.length - 1];
  return { centre, halfKm, planes };
}

export const isAlarm = (a: Aircraft) => a.emergency !== null || a.advisory || a.alert || ['7500', '7600', '7700'].includes(a.squawk ?? '');

/** How long trails stay on the map after the last point, ms. */
export const TRAIL_KEEP_MS = 30 * 60 * 1000;
/** A new point goes on a trail only this far from the last, km: keeps the path short while an aircraft crawls. */
const TRAIL_STEP_KM = 0.3;

export type Trails = Record<string, TrackPoint[]>;

/** `trails` with each aircraft's current position added and old trails dropped; the same object when nothing changed. */
export function extendTrails(trails: Trails, list: Aircraft[], nowMs: number): Trails {
  let out = trails;
  const put = (icao: string, pts: TrackPoint[]) => {
    if (out === trails) out = { ...trails };
    out[icao] = pts;
  };
  for (const a of list) {
    if (a.lat === null || a.lon === null || (a.pos_age_s ?? Infinity) > STALE_POS_S) continue;
    const old = trails[a.icao] ?? [];
    const last = old[old.length - 1];
    if (last && kmBetween({ lat: last.la, lon: last.lo }, { lat: a.lat, lon: a.lon }) < TRAIL_STEP_KM) continue;
    put(a.icao, [...old, { t: nowMs, la: a.lat, lo: a.lon, ...(a.alt_ft !== null ? { a: a.alt_ft } : {}) }]);
  }
  for (const [icao, pts] of Object.entries(trails)) {
    const last = pts[pts.length - 1];
    if (!last || nowMs - last.t > TRAIL_KEEP_MS) {
      if (out === trails) out = { ...trails };
      delete out[icao];
    }
  }
  return out;
}

const pathMemo = new WeakMap<TrackPoint[], { lat: number; lon: number; d: string }>();
/** `trackPath`, kept per track: every status asks for all of them and almost none have changed. */
export function trailPath(pts: TrackPoint[], centre: Site): string {
  const old = pathMemo.get(pts);
  if (old && old.lat === centre.lat && old.lon === centre.lon) return old.d;
  const d = trackPath(pts, centre);
  pathMemo.set(pts, { lat: centre.lat, lon: centre.lon, d });
  return d;
}

/** An SVG path along a track, in the map's km; a jump of over `gapKm` (a stretch not heard) lifts the pen. */
export function trackPath(pts: TrackPoint[], centre: Site, gapKm = 40): string {
  let d = '';
  let prev: TrackPoint | null = null;
  for (const p of pts) {
    const [x, y] = toKm(centre, p.lo, p.la);
    const jump = prev && kmBetween({ lat: prev.la, lon: prev.lo }, { lat: p.la, lon: p.lo }) > gapKm;
    d += `${!prev || jump ? 'M' : 'L'}${x.toFixed(2)} ${y.toFixed(2)}`;
    prev = p;
  }
  return d;
}

/** Trail lines fade to nothing over this long; the picked plane's stays for `TRAIL_KEEP_MS`. */
export const TRAIL_FADE_MS = 15 * 60 * 1000;
const FADE_STEPS = 6;
/** Fades move on this often, so a status that adds no point costs no drawing. */
const FADE_TICK_MS = 60 * 1000;
/** Lower to higher, warm to cool. Yellow means picked and red means alarm, so neither is here. */
export const HEIGHT_BANDS = [
  { under: 2000, label: 'under 2,000', color: 'var(--orange)' },
  { under: 10000, label: '10,000', color: 'var(--green)' },
  { under: 20000, label: '20,000', color: 'var(--cyan)' },
  { under: 30000, label: '30,000', color: 'var(--accent)' },
  { under: Infinity, label: 'higher', color: 'var(--magenta)' },
];

export function heightColor(ft: number | null | undefined): string {
  if (ft === null || ft === undefined) return 'var(--fg-dim)';
  return HEIGHT_BANDS.find((b) => ft < b.under)!.color;
}

export interface TrailPiece {
  d: string;
  color: string;
  opacity: number;
}

/** A track as lines coloured by height and fading with age, joined where colour and fade are the same. */
export function trailPieces(pts: TrackPoint[], centre: Site, nowMs: number, fadeMs = TRAIL_FADE_MS, gapKm = 40): TrailPiece[] {
  const out: TrailPiece[] = [];
  let open: TrailPiece | null = null;
  for (let k = 1; k < pts.length; k++) {
    const [a, b] = [pts[k - 1], pts[k]];
    const left = 1 - (nowMs - b.t) / fadeMs;
    const opacity = Number.isFinite(fadeMs) ? Math.ceil(left * FADE_STEPS) / FADE_STEPS : 1;
    if (opacity <= 0 || kmBetween({ lat: a.la, lon: a.lo }, { lat: b.la, lon: b.lo }) > gapKm) {
      open = null;
      continue;
    }
    const color = heightColor(b.a ?? a.a);
    const [x, y] = toKm(centre, b.lo, b.la);
    const to = `L${x.toFixed(2)} ${y.toFixed(2)}`;
    if (open && open.color === color && open.opacity === opacity) {
      open.d += to;
    } else {
      const [x0, y0] = toKm(centre, a.lo, a.la);
      open = { d: `M${x0.toFixed(2)} ${y0.toFixed(2)}${to}`, color, opacity };
      out.push(open);
    }
  }
  return out;
}

const pieceMemo = new WeakMap<TrackPoint[], { key: string; pieces: TrailPiece[] }>();
/** `trailPieces`, kept per track: every status asks for all of them and almost none have changed. */
export function trailPiecesKept(pts: TrackPoint[], centre: Site, nowMs: number, fadeMs = TRAIL_FADE_MS): TrailPiece[] {
  const key = `${centre.lat},${centre.lon},${fadeMs},${Math.floor(nowMs / FADE_TICK_MS)}`;
  const old = pieceMemo.get(pts);
  if (old?.key === key) return old.pieces;
  const pieces = trailPieces(pts, centre, nowMs, fadeMs);
  pieceMemo.set(pts, { key, pieces });
  return pieces;
}

/** Lines for the planes on the map only: when a plane goes, so does its line. The picked plane keeps its whole track. */
export function tracksToDraw(trails: Trails, planes: { icao: string }[], selected: string | null, centre: Site, nowMs: number): Record<string, TrailPiece[]> {
  const out: Record<string, TrailPiece[]> = {};
  for (const { icao } of planes) {
    const pts = trails[icao];
    if (pts) out[icao] = trailPiecesKept(pts, centre, nowMs, icao === selected ? TRAIL_KEEP_MS : TRAIL_FADE_MS);
  }
  return out;
}

export interface Fact {
  label: string;
  value: string;
}

/** Everything known about an aircraft that the row does not already say; empty values left out. */
export function aircraftFacts(a: Aircraft): Fact[] {
  const hdg = (d: number | null) => (d === null ? '' : `${Math.round(d)}°`);
  const facts: [string, string][] = [
    ['Emergency', a.emergency ?? ''],
    ['Alert', a.advisory ? 'Collision-avoidance advisory' : a.alert ? 'Transponder alert' : ''],
    ['Registration', a.registration ?? ''],
    ['Country', a.country ?? ''],
    ['Type', categoryText(a.category)],
    ['Address', a.icao],
    ['Squawk', a.squawk ?? ''],
    ['State', a.on_ground === null ? '' : a.on_ground ? 'On the ground' : 'In the air'],
    ['Altitude', fmtAlt(a.alt_ft)],
    ['Satellite height', fmtAlt(a.gnss_alt_ft)],
    [a.airspeed ? 'Airspeed' : 'Ground speed', a.speed_kt !== null ? `${Math.round(a.speed_kt)} kt` : ''],
    [a.airspeed ? 'Heading' : 'Track', hdg(a.heading_deg)],
    ['Climb', fmtClimb(a.vrate_fpm)],
    ['Position', a.lat !== null && a.lon !== null ? `${a.lat.toFixed(4)}, ${a.lon.toFixed(4)}` : ''],
    ['Selected altitude', a.selected_alt_ft !== null ? `${fmtAlt(a.selected_alt_ft)}${a.selected_alt_fms ? ' (flight computer)' : ''}` : ''],
    ['Selected heading', hdg(a.selected_heading_deg)],
    ['Altimeter', a.baro_hpa !== null ? `${a.baro_hpa.toFixed(1)} hPa` : ''],
    ['Modes', a.modes.join(', ')],
    ['ADS-B', a.broadcasting ? (a.adsb_version !== null ? `Version ${a.adsb_version}` : 'Broadcasting') : 'Radar replies only'],
    ['Heard', `${a.messages.toLocaleString('en-US')} messages · ${a.snr_db.toFixed(0)} dB`],
  ];
  return facts.filter((f) => f[1]).map(([label, value]) => ({ label, value }));
}

export const flightName = (f: Flight) => f.callsign ?? f.registration ?? f.icao;

/** Type, top altitude, length and messages of a logged flight. */
export function flightLine(f: Flight): string {
  const mins = Math.round((f.last_ms - f.first_ms) / 60000);
  return [
    categoryText(f.category),
    f.emergency ?? '',
    f.max_alt_ft !== null ? `up to ${fmtAlt(f.max_alt_ft)}` : '',
    f.points ? `${mins} min` : 'no position',
    f.messages ? `${f.messages} message${f.messages === 1 ? '' : 's'}` : '',
  ].filter(Boolean).join(' · ');
}

/** Flights whose names, type, squawk or country hold every word of `query`. */
export function filterFlights(list: Flight[], query: string): Flight[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return list;
  return list.filter((f) => {
    const hay = [f.icao, f.callsign, f.registration, f.country, f.category, f.squawk, f.emergency].join(' ').toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

export interface FlightDay {
  day: string;
  flights: Flight[];
}

/** Flights (newest first) under their local day: "Today", "Yesterday", else the date. */
export function flightDays(list: Flight[], nowMs: number): FlightDay[] {
  const key = (ms: number) => new Date(ms).toDateString();
  const today = key(nowMs);
  const yesterday = key(nowMs - 86_400_000);
  const out: FlightDay[] = [];
  for (const f of list) {
    const k = key(f.last_ms);
    const day = k === today ? 'Today' : k === yesterday ? 'Yesterday' : new Date(f.last_ms).toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' });
    const last = out[out.length - 1];
    if (last?.day === day) last.flights.push(f);
    else out.push({ day, flights: [f] });
  }
  return out;
}
