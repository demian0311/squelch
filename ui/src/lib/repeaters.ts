// Repeaters on the map: where each sits from the listener, and whether it is likely in reach.
import type { Band, Repeater } from './api';

/**
 * Typical reach per band, in km. The RepeaterBook export has no power or antenna height,
 * so one figure per band stands in; a mountaintop site reaches much further.
 */
export const REACH_KM: Record<string, number> = { '6m': 60, '2m': 50, '125': 40, '70cm': 30 };

// squelch plays analog FM only; a repeater also listed in one of these may be digital only
const DIGITAL = ['DMR', 'P25', 'NXDN', 'D-STAR', 'M17'];

/** Zoomed all the way out the map shows this far above and below the listener, in km: past it there is nothing to hear. */
export const WIDEST_KM = 75;
/** Zoomed all the way in: repeaters sharing one tower are the only ones still on top of each other. */
export const NEAREST_KM = 1.5;
/** Marks nearer than this on screen fold into one badge. */
export const BUNCH_PX = 22;
/** How far a fanned-out mark sits from the tower it shares, in pixels. */
export const FAN_PX = 24;
/** A badge's radius on screen for this many repeaters, and the radius of a mark kept apart (the tuned one is drawn twice a dot). */
export const badgeRadius = (n: number) => 11 + Math.min(7, n);
export const APART_PX = 12;

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

/**
 * Output frequencies with a signal on them now, with the strongest signal's dB above the noise.
 * A signal under the squelch level (dB above the noise, 0 = off) would be muted, so it does not ring.
 */
export function liveOutputs(points: RepeaterPoint[], signals: { hz: number; snr_db: number }[], squelchDb = 0): Map<number, number> {
  const live = new Map<number, number>();
  for (const s of signals) {
    if (s.snr_db < squelchDb) continue;
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

/** Where the map looks: its centre in km east and south of the listener, and how far it shows above and below the centre. */
export interface MapView {
  x: number;
  y: number;
  halfKm: number;
}
export const DEFAULT_VIEW: MapView = { x: 0, y: 0, halfKm: WIDEST_KM };

/** The view kept between runs, or the default when what was saved is not one. */
export function readView(saved: unknown): MapView {
  const v = saved as Partial<MapView> | null;
  if (!v || typeof v !== 'object' || ![v.x, v.y, v.halfKm].every((n) => typeof n === 'number' && Number.isFinite(n))) return DEFAULT_VIEW;
  return clampView({ x: v.x!, y: v.y!, halfKm: v.halfKm! });
}

/** Never closer than `NEAREST_KM`, never wider than `WIDEST_KM`, and the centre never leaves the widest view. */
export function clampView(v: MapView): MapView {
  const halfKm = Math.min(WIDEST_KM, Math.max(NEAREST_KM, v.halfKm));
  const lim = WIDEST_KM;
  return { x: Math.min(lim, Math.max(-lim, v.x)), y: Math.min(lim, Math.max(-lim, v.y)), halfKm };
}

/**
 * The view zoomed by `factor` (under 1 = closer) so that the place `at` (km) stays where it was on screen,
 * the way a street map zooms around the pointer. Without `at` it zooms around the centre.
 */
export function zoomView(v: MapView, factor: number, at?: { x: number; y: number }): MapView {
  const halfKm = Math.min(WIDEST_KM, Math.max(NEAREST_KM, v.halfKm * factor));
  const f = halfKm / v.halfKm;
  const a = at ?? { x: v.x, y: v.y };
  return clampView({ x: a.x + (v.x - a.x) * f, y: a.y + (v.y - a.y) * f, halfKm });
}

/** What a repeater's colour says: can it be heard here. */
export type Hue = 'green' | 'blue' | 'red';
/** Green: the band should reach it. Blue: also listed under a digital mode, so it may carry no analog FM. Red: probably too far. */
export function hueOf(p: Pick<RepeaterPoint, 'reach' | 'digital'>): Hue {
  return p.digital ? 'blue' : p.reach ? 'green' : 'red';
}

/** One thing drawn on the map: a repeater, or a count badge standing for several too close to tell apart. */
export type Mark =
  | { kind: 'one'; p: RepeaterPoint; x: number; y: number; /** the tower it shares, when fanned out */ from?: { x: number; y: number } }
  | { kind: 'many'; points: RepeaterPoint[]; x: number; y: number; hue: Hue; /** the tower it shares with a mark kept apart, when moved off it */ from?: { x: number; y: number } };

/**
 * Repeaters folded into badges wherever two sit within `bunchPx` of each other on screen, nearest first so a
 * badge forms around the nearest. At the nearest zoom nothing folds: repeaters on one tower fan out around it
 * instead, each a target of its own. Those in `apart` (the tuned one, those on air) never fold: they are what
 * the map is showing. Positions are in km.
 */
export function mapMarks(points: RepeaterPoint[], kmPerPx: number, nearest: boolean, apart: Set<string | null> = new Set(), bunchPx = BUNCH_PX, fanPx = FAN_PX): Mark[] {
  const out: Mark[] = [];
  const used = new Set<number>();
  const cell = bunchPx * kmPerPx;
  for (let i = 0; i < points.length; i++) {
    if (used.has(i)) continue;
    const a = points[i];
    const group = [a];
    used.add(i);
    const sameSite = (b: RepeaterPoint) => Math.abs(a.x - b.x) < 0.05 && Math.abs(a.y - b.y) < 0.05;
    for (let j = i + 1; j < points.length && !apart.has(a.id); j++) {
      if (used.has(j)) continue;
      const b = points[j];
      if (apart.has(b.id)) continue;
      if (nearest ? sameSite(b) : Math.hypot(a.x - b.x, a.y - b.y) < cell) {
        group.push(b);
        used.add(j);
      }
    }
    if (group.length === 1) out.push({ kind: 'one', p: a, x: a.x, y: a.y });
    else if (nearest) {
      const r = fanPx * kmPerPx;
      group.forEach((p, k) => {
        const ang = -Math.PI / 2 + (k * 2 * Math.PI) / group.length;
        out.push({ kind: 'one', p, x: a.x + r * Math.cos(ang), y: a.y + r * Math.sin(ang), from: { x: a.x, y: a.y } });
      });
    } else {
      const x = group.reduce((s, p) => s + p.x, 0) / group.length;
      const y = group.reduce((s, p) => s + p.y, 0) / group.length;
      const hue: Hue = group.some((p) => p.reach && !p.digital) ? 'green' : group.some((p) => p.digital) ? 'blue' : 'red';
      out.push({ kind: 'many', points: group, x, y, hue });
    }
  }
  // a badge on the tower of a mark kept apart would hide under it: it moves beside it, on a line to the tower
  const kept = points.filter((p) => apart.has(p.id));
  for (const m of out) {
    if (m.kind !== 'many') continue;
    const r = badgeRadius(m.points.length);
    const under = kept.find((p) => Math.hypot(p.x - m.x, p.y - m.y) < (r + APART_PX) * kmPerPx);
    if (under) {
      m.from = { x: m.x, y: m.y };
      m.x = under.x + (r + APART_PX + 4) * kmPerPx;
      m.y = under.y;
    }
  }
  return out;
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
