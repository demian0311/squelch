// Pure helpers: frequency text, band lookup, labelling detected signals and placing their cards.
import { ACARS_CENTRE_HZ } from './acars';
import type { Band, BandPlan, Bookmark, Detected, Mode, Rds, Sonde } from './api';
import { channelAt } from './channels';

export const MODE_NAMES: Record<Mode, string> = {
  wfm: 'Wide FM',
  nfm: 'Narrow FM',
  am: 'AM',
  usb: 'USB',
  lsb: 'LSB',
  none: 'No audio',
  p25: 'P25 digital',
  dmr: 'DMR digital',
};

/** 146520000 → "146.520.000" */
export function fmtHz(hz: number): string {
  const s = String(Math.round(hz)).padStart(9, '0');
  const mhz = s.slice(0, -6).replace(/^0+(?=\d)/, '');
  return `${mhz}.${s.slice(-6, -3)}.${s.slice(-3)}`;
}

/** 146520000 → "146.520" */
export function fmtMHz(hz: number, decimals = 3): string {
  return (hz / 1e6).toFixed(decimals);
}

/** The band containing `hz`, preferring the narrowest where bands overlap. */
export function bandAt(plan: BandPlan, hz: number): Band | undefined {
  const mhz = hz / 1e6;
  return plan.bands
    .filter((b) => mhz >= b.low_mhz && mhz <= b.high_mhz)
    .sort((a, b) => a.high_mhz - a.low_mhz - (b.high_mhz - b.low_mhz))[0];
}

/** Percent of the view that lies below and above the band, for shading; 0 where the edge is out of view. */
export function outOfBand(vs: number, ve: number, lo: number, hi: number): { below: number; above: number } {
  const pct = (hz: number) => Math.min(100, Math.max(0, ((hz - vs) / (ve - vs)) * 100));
  return { below: pct(lo), above: 100 - pct(hi) };
}

/** Where to tune on entering a band: the last frequency used there, else its start. */
export function bandEntryHz(b: Band, lastHz: Record<string, number>): number {
  // ACARS decodes channels, not the tuned frequency, and a remembered one can push the busiest out of the capture
  if (b.decoder === 'acars') return ACARS_CENTRE_HZ;
  const hz = lastHz[b.id];
  return hz !== undefined && hz >= b.low_mhz * 1e6 && hz <= b.high_mhz * 1e6 ? hz : Math.round(b.start_mhz * 1e6);
}

/** "2 m", "70 cm": the name says more than an icon five bands share. */
export const bandShort = (b: Band) => (/^[\d.]+ c?m$/.test(b.name) ? b.name.replace(' ', '') : null);

export interface Saved extends Bookmark {
  band: Band;
}

/**
 * Bands that have given us nothing on eagle so far: the whole data group, MURS, railroad, military
 * aircraft and the ham bands except 2 m and 70 cm. Keyed by id so they apply to an old user bands file too.
 * `keep` are in such a group but do have signals: both aircraft decoders, heard live 2026-11-02.
 */
export const DEAD_AIR = { groups: ['data'], bands: ['rail', 'mil', 'murs', '10m', '6m', '125', '33cm', '23cm'], keep: ['adsb', 'acars'] };

/** The band list without the dead-air bands, and without any group left empty. */
export function withoutDeadAir(plan: BandPlan): BandPlan {
  const bands = plan.bands.filter((b) => DEAD_AIR.keep.includes(b.id) || (!DEAD_AIR.groups.includes(b.group) && !DEAD_AIR.bands.includes(b.id)));
  return { ...plan, groups: plan.groups.filter((g) => bands.some((b) => b.group === g.id)), bands };
}

export function allBookmarks(plan: BandPlan): Saved[] {
  const out: Saved[] = [];
  for (const band of plan.bands) for (const b of band.bookmarks) out.push({ ...b, band });
  return out.sort((a, b) => a.mhz - b.mhz);
}

/** Every frequency with a name: the bookmarks, then the built-in names no bookmark replaces. */
export function namedFrequencies(plan: BandPlan, saved: Saved[]): Saved[] {
  const out = [...saved];
  for (const band of plan.bands) {
    for (const n of band.names ?? []) {
      if (!bookmarkNear(saved, n.mhz * 1e6, 1_000)) out.push({ ...n, band });
    }
  }
  return out.sort((a, b) => a.mhz - b.mhz);
}

/** Those in [lo, hi] with no heard signal on them. */
export function notOnAir(list: Saved[], lo: number, hi: number, heard: { hz: number; widthHz: number }[]): Saved[] {
  return list.filter((s) => {
    const hz = s.mhz * 1e6;
    return hz >= lo && hz <= hi && !heard.some((h) => Math.abs(h.hz - hz) <= Math.max(h.widthHz / 2, 5_000));
  });
}

/** The bookmarks of each band that has any, in the rail's order: group by group, then frequency. */
export function bookmarksByBand(plan: BandPlan, saved: Saved[]): { group: { id: string; name: string }; bands: { band: Band; marks: Saved[] }[] }[] {
  return plan.groups
    .map((group) => ({
      group,
      bands: plan.bands.filter((b) => b.group === group.id).map((band) => ({ band, marks: saved.filter((s) => s.band.id === band.id) })).filter((b) => b.marks.length),
    }))
    .filter((g) => g.bands.length);
}

/** Nearest bookmark within `tolHz` of `hz`. */
export function bookmarkNear(saved: Saved[], hz: number, tolHz: number): Saved | undefined {
  let best: Saved | undefined;
  let bestD = tolHz;
  for (const s of saved) {
    const d = Math.abs(s.mhz * 1e6 - hz);
    if (d <= bestD) {
      best = s;
      bestD = d;
    }
  }
  return best;
}

/** The mode to use at `hz`: a named frequency's own mode, else the band's, else `fallback`. */
export function modeAt(plan: BandPlan, named: Saved[], hz: number, fallback: Mode): Mode {
  const mark = bookmarkNear(named, hz, 3_000);
  if (mark?.mode) return mark.mode;
  const band = bandAt(plan, hz);
  return channelAt(band?.id, hz)?.mode ?? band?.mode ?? fallback;
}

export interface Signal {
  hz: number;
  widthHz: number;
  snrDb: number;
  title: string;
  sub: string;
  tag: string;
  saved: boolean;
  /** The fixed channel this signal sits on, for bands with a channel plan. */
  channelHz?: number;
  /** A channel card with nothing on it. */
  quiet?: boolean;
  /** The title is only the frequency, so the header shows the band instead. */
  byFreq?: boolean;
}

/** Name each detected signal from the named frequencies, or describe it. */
export function label(plan: BandPlan, named: Saved[], d: Detected): Signal {
  const tol = Math.max(d.width_hz / 2, 5_000);
  const mark = bookmarkNear(named, d.hz, tol);
  const band = bandAt(plan, d.hz);
  const mode = mark?.mode ?? band?.mode;
  const width = d.width_hz >= 100_000 ? `${Math.round(d.width_hz / 1000)} kHz wide` : `${(d.width_hz / 1000).toFixed(1)} kHz wide`;
  // broadcast FM's measured width only wobbles; it says nothing useful
  const sub = [mode ? MODE_NAMES[mode] : null, mode === 'wfm' ? null : width].filter(Boolean).join(' · ');
  const ch = channelAt(band?.id, d.hz);
  if (mark) return { hz: d.hz, widthHz: d.width_hz, snrDb: d.snr_db, title: mark.name, sub, tag: 'SAVED', saved: true, channelHz: ch?.hz };
  if (ch) return { hz: d.hz, widthHz: d.width_hz, snrDb: d.snr_db, title: ch.name, sub, tag: 'HEARD', saved: false, channelHz: ch.hz };
  if (band?.mode === 'wfm') return { hz: d.hz, widthHz: d.width_hz, snrDb: d.snr_db, title: `FM ${fmtMHz(fmChannel(d.hz), 1)}`, sub, tag: 'HEARD', saved: false, byFreq: true };
  const title = band ? `${band.name} signal` : 'Unknown signal';
  return { hz: d.hz, widthHz: d.width_hz, snrDb: d.snr_db, title, sub, tag: 'HEARD', saved: false };
}

/**
 * Remembers which frequencies have signals over time. A narrow, weak signal that
 * never switches off is usually interference from nearby electronics rather than
 * a transmission, so it is reported as steady and kept off the cards.
 */
export class Persistence {
  private seen = new Map<number, { first: number; hits: number[] }>();
  constructor(
    private bucketHz = 5_000,
    private windowMs = 15_000,
    private minAgeMs = 20_000,
  ) {}

  private key(hz: number) {
    return Math.round(hz / this.bucketHz);
  }

  update(hzs: number[], now: number) {
    for (const hz of hzs) {
      const k = this.key(hz);
      const e = this.seen.get(k) ?? { first: now, hits: [] };
      e.hits.push(now);
      this.seen.set(k, e);
    }
    for (const [k, e] of this.seen) {
      e.hits = e.hits.filter((t) => now - t <= this.windowMs);
      if (!e.hits.length) this.seen.delete(k);
    }
  }

  /** Present for at least `minAgeMs` and in nearly every update of the last window. */
  steady(s: { hz: number; widthHz: number; snrDb: number; saved: boolean }, now: number, updatesPerWindow: number): boolean {
    if (s.saved || s.widthHz > 50_000 || s.snrDb > 25) return false;
    const e = this.seen.get(this.key(s.hz));
    if (!e || now - e.first < this.minAgeMs) return false;
    return e.hits.length >= updatesPerWindow * 0.9;
  }
}

/** A station's steady name: call letters, else the name field. */
export function rdsName(r: Rds): string | null {
  return r.call ?? r.ps;
}

/** What is on: tagged artist and title when both came, else the station's text. */
export function rdsNow(r: Rds): string | null {
  if (r.title && r.artist && r.radiotext?.includes(r.title)) return `${r.artist} – ${r.title}`;
  return r.radiotext ?? (r.pty ? r.pty : null);
}

/** Broadcast FM channel a frequency belongs to (100 kHz steps), for remembering station data. */
export function fmChannel(hz: number): number {
  return Math.round(hz / 100_000) * 100_000;
}

/** Use remembered station data for FM signals. */
export function withRds<T extends Signal>(signals: T[], heard: Map<number, Rds>): T[] {
  return signals.map((sig) => {
    const r = heard.get(fmChannel(sig.hz));
    const name = r && rdsName(r);
    if (!r || !name || sig.saved) return sig;
    return { ...sig, title: `${name} ${(fmChannel(sig.hz) / 1e6).toFixed(1)}`, sub: rdsNow(r) ?? sig.sub, byFreq: false };
  });
}

const COMPASS = ['north', 'north-east', 'east', 'south-east', 'south', 'south-west', 'west', 'north-west'];

/** A direction in words: 0° is north, 90° east. */
export function compassWord(deg: number): string {
  return COMPASS[Math.round((((deg % 360) + 360) % 360) / 45) % 8];
}

const COMPASS_LETTERS = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
/** "SE" for 135°: the letters a compass rose prints. */
export function compassLetters(deg: number): string {
  return COMPASS_LETTERS[Math.round((((deg % 360) + 360) % 360) / 45) % 8];
}

/** "DFM-17 23038743", or the type alone until the serial arrives. */
export function sondeName(s: Sonde): string {
  return s.serial ? `${s.kind} ${s.serial}` : `${s.kind} balloon`;
}

/** One line on height and movement: "12.3 km · rising 5.1 m/s". */
export function sondeSummary(s: Sonde): string {
  if (s.alt_m === null) return 'Heard, no GPS position yet';
  const km = `${(s.alt_m / 1000).toFixed(1)} km`;
  const c = s.climb_ms ?? 0;
  const move = Math.abs(c) < 0.5 ? 'level' : c > 0 ? `rising ${c.toFixed(1)} m/s` : `falling ${(-c).toFixed(1)} m/s`;
  return `${km} · ${move}`;
}

/** "drifting east at 18 m/s" */
export function sondeDrift(s: Sonde): string | null {
  if (s.speed_ms === null || s.heading_deg === null) return null;
  const dir = COMPASS[Math.round(s.heading_deg / 45) % 8];
  return `drifting ${dir} at ${Math.round(s.speed_ms)} m/s`;
}

/** Name detected signals after the balloon decoded on them. */
export function withSondes<T extends Signal>(signals: T[], sondes: Sonde[]): T[] {
  return signals.map((sig) => {
    const s = sondes.find((o) => Math.abs(o.hz - sig.hz) < 10_000);
    return s ? { ...sig, title: sondeName(s), sub: sondeSummary(s), tag: 'BALLOON', saved: true } : sig;
  });
}

/**
 * A signal's measured centre wobbles by a few kHz between updates. Keep the previous anchor
 * while the new centre stays within `tolHz` of it, so cards and their digits hold still.
 */
export function anchor(hz: number, prev: number[], tolHz: number): number {
  let best: number | undefined;
  for (const p of prev) if (Math.abs(p - hz) <= tolHz && (best === undefined || Math.abs(p - hz) < Math.abs(best - hz))) best = p;
  return best ?? hz;
}

export interface Placed<T> {
  item: T;
  hidden: T[];
}

/**
 * Give each signal a card at its frequency. Where cards would overlap, the
 * strongest signal keeps the card and the weaker ones are folded into it.
 * `first` (the one being listened to) always wins. `minSepHz` is how far apart two cards' centres must be.
 */
export function placeCards<T extends { hz: number; snrDb: number }>(items: T[], minSepHz: number | ((a: T, b: T) => number), first?: (t: T) => boolean): Placed<T>[] {
  const sep = typeof minSepHz === 'number' ? () => minSepHz : minSepHz;
  const order = [...items].sort((a, b) => {
    const fa = first?.(a) ? 1 : 0;
    const fb = first?.(b) ? 1 : 0;
    return fb - fa || b.snrDb - a.snrDb;
  });
  const placed: Placed<T>[] = [];
  for (const it of order) {
    const hit = placed.find((p) => Math.abs(p.item.hz - it.hz) < sep(p.item, it));
    if (hit) hit.hidden.push(it);
    else placed.push({ item: it, hidden: [] });
  }
  return placed.sort((a, b) => a.item.hz - b.item.hz);
}

/**
 * Where the dongle should sit for this view: `center` while the view and the tuned frequency are within `half`
 * of it. Otherwise the middle of both, which leaves the most room before the next move; when both cannot fit,
 * the view wins and the audio waits. The tuned frequency stays clear of the dongle's DC spike by `dcOffset`.
 */
export function captureCenter(center: number, vs: number, ve: number, tuned: number, half: number, dcOffset: number): number {
  const lo = Math.min(vs, tuned);
  const hi = Math.max(ve, tuned);
  const fits = hi - lo <= 2 * half;
  const inside = (f: number) => f >= center - half && f <= center + half;
  if (inside(vs) && inside(ve) && (inside(tuned) || !fits)) return center;
  const c = fits ? (lo + hi) / 2 : (vs + ve) / 2;
  return Math.round(Math.abs(tuned - c) < dcOffset ? tuned + dcOffset : c);
}

/** Snap distance for clicks, in screen pixels. Further away, a click tunes exactly where it lands. */
export const SNAP_PX = 8;

/** Pick the frequency a click lands on: a named frequency or signal centre within SNAP_PX, else the click itself. */
export function snapTune(hz: number, signals: Detected[], named: Saved[], pxHz: number, mode: Mode, bandId?: string): number {
  const reach = SNAP_PX * pxHz;
  const mark = bookmarkNear(named, hz, reach);
  if (mark) return mark.mhz * 1e6;
  // on a channel plan, a click near a channel or a signal on one tunes the channel
  const ch = channelAt(bandId, hz);
  if (ch && Math.abs(ch.hz - hz) <= Math.max(reach, 1_000)) return ch.hz;
  const near = signals.filter((s) => Math.abs(s.hz - hz) <= reach).sort((a, b) => Math.abs(a.hz - hz) - Math.abs(b.hz - hz))[0];
  if (near) {
    const onCh = channelAt(bandId, near.hz);
    if (onCh) return onCh.hz;
    // broadcast FM sits on 100 kHz channels
    if (mode === 'wfm') return Math.round(near.hz / 100_000) * 100_000;
    return Math.round(near.hz / 500) * 500;
  }
  const step = mode === 'wfm' ? 10_000 : mode === 'usb' || mode === 'lsb' ? 10 : 500;
  return Math.round(hz / step) * step;
}

/** The FFT bin holding a frequency. Bin i is centred on lo + i * binHz (lo = capture's low edge). */
export const binAt = (hz: number, lo: number, binHz: number) => Math.floor((hz - lo) / binHz + 0.5);
