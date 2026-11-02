import type { Rds } from './api';
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
export function noteHeard(list: Heard[], signals: { hz: number; snr_db: number }[], names: Map<number, Rds>, now: number): Heard[] {
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
    if (old && now - old.at < TOUCH_MS && (!name || name === old.name) && (!type || type === old.type)) continue;
    next[i >= 0 ? i : next.length] = { hz: ch, at: now, snrDb: Math.round(s.snr_db), name: name ?? old?.name, type: type ?? old?.type };
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
export function withHeard<T extends Signal>(signals: T[], list: Heard[]): T[] {
  return signals.map((sig) => {
    const name = heardName(list, sig.hz);
    if (!name || sig.saved || !sig.byFreq) return sig;
    return { ...sig, title: `${name} ${(fmChannel(sig.hz) / 1e6).toFixed(1)}`, byFreq: false };
  });
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
