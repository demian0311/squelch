// Pure helpers: frequency text, band lookup, labelling detected signals and placing their cards.
import type { Band, BandPlan, Bookmark, Detected, Mode } from './api';

export const MODE_NAMES: Record<Mode, string> = {
  wfm: 'Wide FM',
  nfm: 'Narrow FM',
  am: 'AM',
  usb: 'USB',
  lsb: 'LSB',
  none: 'No audio',
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

export interface Saved extends Bookmark {
  band: Band;
}

export function allBookmarks(plan: BandPlan): Saved[] {
  const out: Saved[] = [];
  for (const band of plan.bands) for (const b of band.bookmarks) out.push({ ...b, band });
  return out.sort((a, b) => a.mhz - b.mhz);
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

/** The mode to use at `hz`: a bookmark's own mode, else the band's, else `fallback`. */
export function modeAt(plan: BandPlan, saved: Saved[], hz: number, fallback: Mode): Mode {
  const mark = bookmarkNear(saved, hz, 3_000);
  if (mark?.mode) return mark.mode;
  return bandAt(plan, hz)?.mode ?? fallback;
}

export interface Signal {
  hz: number;
  widthHz: number;
  snrDb: number;
  title: string;
  sub: string;
  tag: string;
  saved: boolean;
}

/** Name each detected signal from the bookmarks, or describe it. */
export function label(plan: BandPlan, saved: Saved[], d: Detected): Signal {
  const tol = Math.max(d.width_hz / 2, 5_000);
  const mark = bookmarkNear(saved, d.hz, tol);
  const band = bandAt(plan, d.hz);
  const mode = mark?.mode ?? band?.mode;
  const width = d.width_hz >= 100_000 ? `${Math.round(d.width_hz / 1000)} kHz wide` : `${(d.width_hz / 1000).toFixed(1)} kHz wide`;
  const sub = [mode ? MODE_NAMES[mode] : null, width].filter(Boolean).join(' · ');
  if (mark) return { hz: d.hz, widthHz: d.width_hz, snrDb: d.snr_db, title: mark.name, sub, tag: 'SAVED', saved: true };
  const title = band?.mode === 'wfm' ? `FM ${fmtMHz(Math.round(d.hz / 1e5) * 1e5, 1)}` : band ? `${band.name} signal` : 'Unknown signal';
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

export interface Placed<T> {
  item: T;
  hidden: T[];
}

/**
 * Give each signal a card at its frequency. Where cards would overlap, the
 * strongest signal keeps the card and the weaker ones are folded into it.
 * `first` (the one being listened to) always wins.
 */
export function placeCards<T extends { hz: number; snrDb: number }>(items: T[], minSepHz: number, first?: (t: T) => boolean): Placed<T>[] {
  const order = [...items].sort((a, b) => {
    const fa = first?.(a) ? 1 : 0;
    const fb = first?.(b) ? 1 : 0;
    return fb - fa || b.snrDb - a.snrDb;
  });
  const placed: Placed<T>[] = [];
  for (const it of order) {
    const hit = placed.find((p) => Math.abs(p.item.hz - it.hz) < minSepHz);
    if (hit) hit.hidden.push(it);
    else placed.push({ item: it, hidden: [] });
  }
  return placed.sort((a, b) => a.item.hz - b.item.hz);
}

/** Pick the frequency a click lands on: a nearby signal, a bookmark, or a rounded step. */
export function snapTune(hz: number, signals: Detected[], saved: Saved[], pxHz: number, mode: Mode): number {
  const near = signals
    .filter((s) => Math.abs(s.hz - hz) <= Math.max(s.width_hz / 2, 6 * pxHz))
    .sort((a, b) => Math.abs(a.hz - hz) - Math.abs(b.hz - hz))[0];
  const mark = bookmarkNear(saved, near?.hz ?? hz, Math.max(6 * pxHz, 3_000));
  if (mark) return mark.mhz * 1e6;
  if (near) {
    // broadcast FM sits on 100 kHz steps; narrow signals on 2.5 kHz
    if (mode === 'wfm') return Math.round(near.hz / 100_000) * 100_000;
    return Math.round(near.hz / 2_500) * 2_500;
  }
  const step = mode === 'wfm' ? 100_000 : mode === 'usb' || mode === 'lsb' ? 100 : 2_500;
  return Math.round(hz / step) * step;
}
