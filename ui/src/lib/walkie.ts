// Walkie-talkies: FRS and GMRS share channels 1–22 (47 CFR 95.563 / 95.1763); only the power allowed differs.
// GMRS repeaters listen 5 MHz above 15–22 and repeat on them; those inputs are "15R"–"22R".
// The dongle hears 2.4 MHz at once, so the 462 MHz side (1–7, 15–22) and the 467 MHz side (8–14, inputs) are heard
// one at a time. While either side is in the capture, each minute records which channels were on air; the last
// KEEP_MIN minutes are kept (localStorage `walkie.history`) and drawn as the Activity tab.

export const WALKIE_BANDS = new Set(['walkie', 'frs_only', 'gmrs']);

const MHZ = [462.5625, 462.5875, 462.6125, 462.6375, 462.6625, 462.6875, 462.7125, 467.5625, 467.5875, 467.6125, 467.6375, 467.6625, 467.6875, 467.7125, 462.55, 462.575, 462.6, 462.625, 462.65, 462.675, 462.7, 462.725];
const INPUT_SHIFT_HZ = 5_000_000;
// channels sit 12.5 kHz apart
const REACH_HZ = 6_250;

export interface WalkieChannel {
  n: number;
  /** A repeater input: its users talking to the repeater. */
  input: boolean;
  hz: number;
  /** "18", or "18R" for an input. */
  key: string;
}

export const WALKIE: WalkieChannel[] = [
  ...MHZ.map((m, i) => ({ n: i + 1, input: false, hz: Math.round(m * 1e6), key: String(i + 1) })),
  ...MHZ.slice(14).map((m, i) => ({ n: i + 15, input: true, hz: Math.round(m * 1e6) + INPUT_SHIFT_HZ, key: `${i + 15}R` })),
];
export const walkieByKey = new Map(WALKIE.map((c) => [c.key, c]));

export interface Strip {
  name: string;
  from: number;
  to: number;
  /** Power allowed, FRS then GMRS (47 CFR 95.567 / 95.1767). */
  power: string;
}
export const STRIPS: Strip[] = [
  { name: 'Everyday', from: 1, to: 7, power: 'FRS 2 W · GMRS 5 W' },
  { name: 'Short range', from: 8, to: 14, power: '0.5 W hand-helds' },
  { name: 'Repeaters', from: 15, to: 22, power: 'FRS 2 W · GMRS 50 W' },
];
export const stripOf = (n: number) => STRIPS.find((s) => n >= s.from && n <= s.to)!;
export const isRepeater = (n: number) => n >= 15;

export type Side = 462 | 467;
export const sideOf = (hz: number): Side => (hz < 465e6 ? 462 : 467);
const SIDE_BIT: Record<Side, number> = { 462: 1, 467: 2 };
const SPAN: Record<Side, [number, number]> = { 462: [462_550_000, 462_725_000], 467: [467_550_000, 467_725_000] };

/** The sides whose every channel lies between `lo` and `hi`. */
export function sidesIn(lo: number, hi: number): Side[] {
  return ([462, 467] as Side[]).filter((s) => SPAN[s][0] - REACH_HZ >= lo && SPAN[s][1] + REACH_HZ <= hi);
}

/** The channel a signal at `hz` is on, if any. */
export function walkieAt(hz: number): WalkieChannel | undefined {
  let best: WalkieChannel | undefined;
  for (const c of WALKIE) if (!best || Math.abs(c.hz - hz) < Math.abs(best.hz - hz)) best = c;
  return best && Math.abs(best.hz - hz) <= REACH_HZ ? best : undefined;
}

/** Strongest signal per channel key, dB above the floor. */
export function liveWalkies(signals: { hz: number; snr_db?: number }[]): Map<string, number> {
  const out = new Map<string, number>();
  for (const s of signals) {
    const c = walkieAt(s.hz);
    if (c) out.set(c.key, Math.max(out.get(c.key) ?? -Infinity, s.snr_db ?? 0));
  }
  return out;
}

export const KEEP_MIN = 6 * 60;
/** Minutes listened before the Activity tab is offered. */
export const SHOW_AFTER_MIN = 5;
/** The most columns drawn; longer histories put several minutes in a column. */
export const COLUMNS = 60;

export interface Minute {
  /** Unix time in minutes. */
  t: number;
  /** Sides listened to during the minute, bits 1 = 462, 2 = 467. */
  sides: number;
  /** Channel keys heard during the minute. */
  on: string[];
}

/**
 * Adds what is heard now to the minute holding `ms`, and drops minutes older than KEEP_MIN. Returns true when this
 * started a new minute, so the finished one can be saved.
 */
export function record(hist: Minute[], ms: number, sides: Side[], live: Iterable<string>): boolean {
  const t = Math.floor(ms / 60_000);
  let m = hist.at(-1);
  const started = !m || m.t !== t;
  if (started) {
    m = { t, sides: 0, on: [] };
    hist.push(m);
    while (hist.length && hist[0].t <= t - KEEP_MIN) hist.shift();
  }
  for (const s of sides) m!.sides |= SIDE_BIT[s];
  for (const k of live) {
    const c = walkieByKey.get(k);
    if (c && m!.sides & SIDE_BIT[sideOf(c.hz)] && !m!.on.includes(k)) m!.on.push(k);
  }
  return started;
}

export function parseHistory(text: string, nowMs: number): Minute[] {
  try {
    const t = Math.floor(nowMs / 60_000);
    const list = JSON.parse(text);
    if (!Array.isArray(list)) return [];
    return list.filter((m): m is Minute => typeof m?.t === 'number' && typeof m.sides === 'number' && Array.isArray(m.on) && m.t > t - KEEP_MIN && m.t <= t);
  } catch {
    return [];
  }
}

/** Minutes in the window during which either side was listened to. */
export const listened = (hist: Minute[]) => hist.filter((m) => m.sides).length;

export interface ActivityRow {
  channel: WalkieChannel;
  /** Per column: null = its side was not listened to, else the share of listened minutes it was on air (0–1). */
  cells: (number | null)[];
  /** Unix minute it was last heard. */
  last: number | null;
}

export interface Activity {
  /** Minutes the columns cover, ending now. */
  spanMin: number;
  rows: ActivityRow[];
}

/** The history as a grid: one row per channel, columns from the first listened minute (at most KEEP_MIN ago) to now. */
export function activity(hist: Minute[], nowMs: number): Activity {
  const now = Math.floor(nowMs / 60_000);
  const first = hist.find((m) => m.sides)?.t ?? now;
  const spanMin = Math.min(KEEP_MIN, Math.max(SHOW_AFTER_MIN, now - first + 1));
  const cols = Math.min(COLUMNS, spanMin);
  const start = now - spanMin + 1;
  const col = (t: number) => Math.min(cols - 1, Math.floor(((t - start) * cols) / spanMin));
  const rows = WALKIE.map((channel) => {
    const bit = SIDE_BIT[sideOf(channel.hz)];
    const heard = new Array<number>(cols).fill(0);
    const minutes = new Array<number>(cols).fill(0);
    let last: number | null = null;
    for (const m of hist) {
      if (m.t < start || !(m.sides & bit)) continue;
      const c = col(m.t);
      minutes[c]++;
      if (m.on.includes(channel.key)) {
        heard[c]++;
        last = m.t;
      }
    }
    return { channel, cells: minutes.map((n, i) => (n ? heard[i] / n : null)), last };
  });
  return { spanMin, rows };
}

/** "now", "4 min", "2 h 10 min" since unix minute `t`. */
export function ago(t: number | null, nowMs: number): string {
  if (t === null) return '';
  const d = Math.floor(nowMs / 60_000) - t;
  if (d <= 0) return 'now';
  if (d < 60) return `${d} min`;
  return `${Math.floor(d / 60)} h${d % 60 ? ` ${d % 60} min` : ''}`;
}

/** "45 min" or "2 h 30 min" for a span. */
export const spanText = (min: number) => (min < 60 ? `${min} min` : `${Math.floor(min / 60)} h${min % 60 ? ` ${min % 60} min` : ''}`);
