import type { FmStation, Rds } from './api';
import { fmChannel, rdsName, type Signal } from './radio';

/** A station heard on a band, remembered across runs so the band strip can show the whole band. */
export interface Heard {
  hz: number;
  /** Last time it was heard, ms since the epoch. */
  at: number;
  snrDb: number;
  name?: string;
  /** What the station says it plays (RDS programme type): News, Classical. */
  type?: string;
  /** Last seen with HD Radio sidebands, or not. */
  hd?: boolean;
}

/** A station not heard for this long drops off the strip. */
export const MAX_AGE_MS = 30 * 24 * 3600 * 1000;
/** A station still on air is re-stamped at most this often, so storage isn't written per status. */
export const TOUCH_MS = 60_000;
/** How far off an FM channel a signal may sit and still be that station (the engine already drops HD sidebands). */
const GRID_SLACK_HZ = 20_000;
/** Weaker than this is noise on the strip. */
export const MIN_SNR_DB = 6;

/**
 * Records FM stations among `signals` in `list`. Returns `list` itself when nothing is worth storing,
 * so the caller writes only on change.
 */
export function noteHeard(list: Heard[], signals: { hz: number; snr_db: number; hd?: boolean | null }[], names: Map<number, Rds>, now: number): Heard[] {
  let changed = false;
  const next = list.filter((h) => now - h.at <= MAX_AGE_MS);
  if (next.length !== list.length) changed = true;
  for (const s of signals) {
    const ch = fmChannel(s.hz);
    if (Math.abs(s.hz - ch) > GRID_SLACK_HZ || s.snr_db < MIN_SNR_DB) continue;
    const r = names.get(ch);
    const name = (r && rdsName(r)) ?? undefined;
    const type = r?.pty ?? undefined;
    const i = next.findIndex((h) => h.hz === ch);
    const old = i >= 0 ? next[i] : undefined;
    if (old && now - old.at < TOUCH_MS && (!name || name === old.name) && (!type || type === old.type) && (s.hd == null || old.hd !== undefined)) continue;
    next[i >= 0 ? i : next.length] = { hz: ch, at: now, snrDb: Math.round(s.snr_db), name: name ?? old?.name, type: type ?? old?.type, hd: s.hd ?? old?.hd };
    changed = true;
  }
  return changed ? next.sort((a, b) => a.hz - b.hz) : list;
}

/** What a remembered station plays, for the channel `hz` is on. */
export function heardType(list: Heard[], hz: number): string | undefined {
  return list.find((h) => h.hz === fmChannel(hz))?.type;
}

/** A remembered station's name, for the channel `hz` is on. */
export function heardName(list: Heard[], hz: number): string | undefined {
  return list.find((h) => h.hz === fmChannel(hz))?.name;
}

/** Names FM signals still known only by frequency from earlier runs, until RDS comes in this run. */
export function withHeard<T extends Signal>(signals: T[], nameAt: (hz: number) => string | undefined): T[] {
  return signals.map((sig) => {
    const name = nameAt(sig.hz);
    if (!name || sig.saved || !sig.byFreq) return sig;
    return { ...sig, title: `${name} ${(fmChannel(sig.hz) / 1e6).toFixed(1)}`, byFreq: false };
  });
}

/** Stronger than this, a signal on an FM channel with no station on the list is still worth a row. */
export const LISTED_SNR_DB = 15;
const isChannel = (hz: number) => Math.round(hz / 100_000) % 2 === 1;

/**
 * The station on each channel near here. One the radio has named wins (a station past its guessed reach needs that
 * to list at all), else the nearest in reach. `stations` comes nearest first. Memory is keyed by channel and knows
 * nothing of where it was heard, so a name the list does not hold on that channel is ignored.
 */
export function localStations(stations: FmStation[], heard: Heard[], names: Map<number, Rds>): Map<number, FmStation> {
  const out = new Map<number, FmStation>();
  for (const s of stations) if (s.in_reach && !out.has(s.hz)) out.set(s.hz, s);
  for (const s of stations) {
    const r = names.get(s.hz);
    if ((r && rdsName(r) === s.call) || heardName(heard, s.hz) === s.call) out.set(s.hz, s);
  }
  return out;
}

/** A channel's name: the list's when there is a list, else what memory holds. */
export function localName(hasList: boolean, local: Map<number, FmStation>, heard: Heard[], hz: number): string | undefined {
  return hasList ? local.get(fmChannel(hz))?.call : heardName(heard, hz);
}

/** What a channel plays, from memory, when memory is of this station. */
export function localType(hasList: boolean, local: Map<number, FmStation>, heard: Heard[], hz: number): string | undefined {
  const h = heard.find((x) => x.hz === fmChannel(hz));
  if (!hasList) return h?.type;
  return h?.name && h.name === local.get(fmChannel(hz))?.call ? h.type : undefined;
}

export interface DialRow {
  hz: number;
  title: string;
  city?: string;
  type?: string;
  hd: boolean;
  /** Live inside the capture, else the last reading; null when nothing was heard of it here. */
  snrDb: number | null;
  onAir: boolean;
}

/** One row per station on the dial. `since`: when this run started; older readings without a matching name may be from elsewhere. */
export function dialRows(hasList: boolean, local: Map<number, FmStation>, heard: Heard[], signals: { hz: number; snr_db: number; hd?: boolean | null }[], names: Map<number, Rds>, since: number): DialRow[] {
  const live = new Map<number, { snr_db: number; hd?: boolean | null }>();
  for (const s of signals) {
    const ch = fmChannel(s.hz);
    if (Math.abs(s.hz - ch) <= GRID_SLACK_HZ && s.snr_db >= MIN_SNR_DB) live.set(ch, s);
  }
  const channels = new Set<number>(local.keys());
  for (const [ch, s] of live) if (isChannel(ch) && (s.snr_db >= LISTED_SNR_DB || names.has(ch))) channels.add(ch);
  if (!hasList) for (const h of heard) if (isChannel(h.hz) && (h.name || h.snrDb >= LISTED_SNR_DB)) channels.add(h.hz);
  // runs on every status, across every station: one lookup each, not a search of the list
  const memory = new Map(heard.map((h) => [h.hz, h]));
  const rows = [...channels].map((hz): DialRow => {
    const st = local.get(hz);
    const r = names.get(hz);
    const h = memory.get(hz);
    const s = live.get(hz);
    const mine = !!h && (!hasList || h.at >= since || (!!st && h.name === st.call));
    const name = (r && rdsName(r)) ?? (hasList ? st?.call : h?.name);
    // what it plays, from memory only when memory is of this station; same rule as `localType`
    const memoryType = !hasList || (h?.name && h.name === st?.call) ? h?.type : undefined;
    return {
      hz,
      title: name ?? `FM ${(hz / 1e6).toFixed(1)}`,
      city: st?.city,
      type: r?.pty ?? memoryType,
      hd: s?.hd ?? (mine ? h!.hd : undefined) ?? false,
      snrDb: s ? Math.round(s.snr_db) : mine ? h!.snrDb : null,
      onAir: !!s,
    };
  });
  return rows.sort((a, b) => a.hz - b.hz);
}

export type DialSort = 'freq' | 'call' | 'city' | 'signal';
/** Signals this close sort as equal, so a row doesn't jump with every status. */
export const SORT_DB = 3;
/** How a column sorts when first clicked: strongest first, everything else from the top of the alphabet or dial. */
export const firstDesc = (key: DialSort) => key === 'signal';

/** A copy of `rows` by that column. Rows with nothing in it stay last either way; ties go by frequency. */
export function sortDial(rows: DialRow[], key: DialSort, desc: boolean): DialRow[] {
  const sign = desc ? -1 : 1;
  const missing = (r: DialRow) => (key === 'signal' ? r.snrDb === null : key === 'city' ? !r.city : false);
  const by = (a: DialRow, b: DialRow) => {
    if (key === 'freq') return a.hz - b.hz;
    if (key === 'signal') return Math.round((a.snrDb ?? 0) / SORT_DB) - Math.round((b.snrDb ?? 0) / SORT_DB);
    return (key === 'city' ? (a.city ?? '') : a.title).localeCompare(key === 'city' ? (b.city ?? '') : b.title);
  };
  return [...rows].sort((a, b) => Number(missing(a)) - Number(missing(b)) || sign * by(a, b) || a.hz - b.hz);
}

export interface Tick {
  hz: number;
  title: string;
  /** Heard since the app started; older ones are from earlier runs and may be gone. */
  fresh: boolean;
  /** 0–1, for the tick's height. */
  strength: number;
  saved: boolean;
}

/** Strip ticks: remembered stations plus bookmarks in the band; a bookmark on a station's channel merges into it. */
export function stripTicks(heard: Heard[], saved: { mhz: number; name: string }[], since: number, lo: number, hi: number): Tick[] {
  const ticks: Tick[] = heard
    .filter((h) => h.hz >= lo && h.hz <= hi)
    .map((h) => ({ hz: h.hz, title: `${(h.hz / 1e6).toFixed(1)}${h.name ? ' ' + h.name : ''}`, fresh: h.at >= since, strength: Math.min(1, Math.max(0, h.snrDb / 40)), saved: false }));
  for (const s of saved) {
    const hz = s.mhz * 1e6;
    if (hz < lo || hz > hi) continue;
    const t = ticks.find((t) => Math.abs(t.hz - hz) < GRID_SLACK_HZ);
    if (t) {
      t.saved = true;
      t.title = `${(t.hz / 1e6).toFixed(1)} ${s.name}`;
    } else ticks.push({ hz, title: `${s.mhz.toFixed(1)} ${s.name}`, fresh: false, strength: 0.3, saved: true });
  }
  return ticks.sort((a, b) => a.hz - b.hz);
}

/** A click this close to a tick picks it. */
export const TICK_HIT_PX = 8;

/** The tick a click at `hz` lands on, if any. */
export function pickTick(ticks: Tick[], hz: number, hzPerPx: number): Tick | null {
  let best: Tick | null = null;
  for (const t of ticks) if (Math.abs(t.hz - hz) <= TICK_HIT_PX * hzPerPx && (!best || Math.abs(t.hz - hz) < Math.abs(best.hz - hz))) best = t;
  return best;
}

/** Scale labels: the smallest round MHz step at least `minPx` apart. */
export function stripLabels(lo: number, hi: number, width: number, minPx = 70): number[] {
  const steps = [0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100].map((m) => m * 1e6);
  const step = steps.find((s) => (s / (hi - lo)) * width >= minPx) ?? steps[steps.length - 1];
  const out: number[] = [];
  for (let f = Math.ceil(lo / step) * step; f <= hi; f += step) out.push(f);
  return out;
}
