// Bands whose radios pick numbered channels, as tiles grouped by what each channel is for. A band is data here; the
// panel (`ChannelBoard.svelte`), the Activity tab and the waterfall's channel plan (`channels.ts`) all read it.
// Sources (checked 2026-11-02): CB 47 CFR 95.931, channels 19 and 36-40 by custom (Wikipedia, Citizens band radio);
// marine 47 CFR 80.373(f) and 80.371(c); MURS 47 CFR 95.2763, Blue and Green Dot by custom; FRS and GMRS 95.563 / 95.1763;
// NOAA weather.gov/nwr. GMRS repeaters listen 5 MHz above 15-22; those inputs are "15R"-"22R".
//
// The dongle hears 2.4 MHz at once, so a band wider than that has sides heard one at a time (walkie 462 / 467 MHz, marine
// ship / shore, MURS 151 / 154 MHz). While a side is in the capture, each minute records which channels were on air; the
// last KEEP_MIN minutes are kept (`<board id>.history` in the state file) and drawn as the Activity tab.

import type { Mode } from './api';

export interface BoardChannel {
  /** Shown on the tile and in history: "18", "18R", "24". */
  key: string;
  hz: number;
  /** Names it in the detail line when the key alone doesn't: "24 shore". */
  name?: string;
  /** What it is for, in a word or two, on the tile. */
  word?: string;
  /** What it is for, in the detail line. */
  use: string;
  /** Tuning it switches to this mode, whatever the band's own. */
  mode?: Mode;
  /** Marked with a dot on its tile: a walkie-talkie repeater channel. */
  dot?: boolean;
  /** The key of the channel's other frequency. */
  twin?: string;
  /** Reached through its twin; not a tile of its own. */
  second?: boolean;
}

export interface Strip {
  name: string;
  /** Right of the heading: power, bandwidth. */
  note?: string;
  keys: string[];
}

export interface Board {
  id: string;
  title: string;
  strips: Strip[];
  /** In plan order: this is also the band's channel plan. */
  channels: BoardChannel[];
  /** Lowest and highest channel per side, Hz. Each must fit one capture. */
  sides: [number, number][];
  /** A signal this close is on the channel: half the tightest spacing. */
  reachHz: number;
  /** Where to tune the first time, instead of the band's start. */
  home?: number;
  /** A tile with a twin is two halves with these labels; without it, the detail line links to the twin. */
  halves?: [string, string];
  /** The detail line's links to the twin and back. */
  twinLinks?: [string, string];
  /** Heading for second channels in the Activity tab. */
  secondName?: string;
  /** Shown when the tuned frequency is on no channel. */
  hint: string;
}

const hzOf = (m: number) => Math.round(m * 1e6);
const span = (list: { hz: number }[]): [number, number] => [Math.min(...list.map((c) => c.hz)), Math.max(...list.map((c) => c.hz))];

// ── walkie-talkies ──
const WALKIE_MHZ = [462.5625, 462.5875, 462.6125, 462.6375, 462.6625, 462.6875, 462.7125, 467.5625, 467.5875, 467.6125, 467.6375, 467.6625, 467.6875, 467.7125, 462.55, 462.575, 462.6, 462.625, 462.65, 462.675, 462.7, 462.725];
const INPUT_SHIFT_HZ = 5_000_000;
const WALKIE_POWER = ['FRS 2 W · GMRS 5 W', '0.5 W hand-helds', 'FRS 2 W · GMRS 50 W'];
const walkieChannels: BoardChannel[] = [
  ...WALKIE_MHZ.map((m, i): BoardChannel => {
    const n = i + 1;
    if (n >= 15) return { key: String(n), hz: hzOf(m), dot: true, twin: `${n}R`, use: `Repeater output, or people talking directly. ${WALKIE_POWER[2]}.` };
    return { key: String(n), hz: hzOf(m), use: n >= 8 ? `Short range: hand-helds at 0.5 W only. ${WALKIE_POWER[1]}.` : `Everyday channel. ${WALKIE_POWER[0]}.` };
  }),
  ...WALKIE_MHZ.slice(14).map((m, i): BoardChannel => {
    const n = i + 15;
    return { key: `${n}R`, hz: hzOf(m) + INPUT_SHIFT_HZ, dot: true, second: true, twin: String(n), use: `People talking into the channel ${n} repeater, heard before it repeats them.` };
  }),
];
const keysOf = (from: number, to: number) => Array.from({ length: to - from + 1 }, (_, i) => String(from + i));
const WALKIE_BOARD: Board = {
  id: 'walkie',
  title: 'Walkie-talkies',
  strips: [
    { name: 'Everyday', note: WALKIE_POWER[0], keys: keysOf(1, 7) },
    { name: 'Short range', note: WALKIE_POWER[1], keys: keysOf(8, 14) },
    { name: 'Repeaters', note: WALKIE_POWER[2], keys: keysOf(15, 22) },
  ],
  channels: walkieChannels,
  sides: [[462_550_000, 462_725_000], [467_550_000, 467_725_000]],
  reachHz: 6_250,
  twinLinks: ['Listen to its users on', 'Back to the repeater'],
  secondName: 'Repeater users, 5 MHz up',
  hint: 'Click a channel to tune it. FRS and GMRS radios use the same 22 channels.',
};

// ── CB ──
const CB = [26.965, 26.975, 26.985, 27.005, 27.015, 27.025, 27.035, 27.055, 27.065, 27.075, 27.085, 27.105, 27.115, 27.125, 27.135, 27.155, 27.165, 27.175, 27.185, 27.205, 27.215, 27.225, 27.255, 27.235, 27.245, 27.265, 27.275, 27.285, 27.295, 27.305, 27.315, 27.325, 27.335, 27.345, 27.355, 27.365, 27.375, 27.385, 27.395, 27.405];
const cbChannels = CB.map((m, i): BoardChannel => {
  const n = i + 1;
  const c: BoardChannel = { key: String(n), hz: hzOf(m), use: 'Everyday talk, AM.' };
  if (n === 9) return { ...c, word: 'Emergency', use: 'Emergencies and traveler help only, by FCC rule.' };
  if (n === 19) return { ...c, word: 'Highway', use: 'The highway channel, by custom. Most truckers and road travelers listen here.' };
  if (n === 38) return { ...c, word: 'LSB', mode: 'lsb', use: 'By custom, sideband calling on lower sideband.' };
  if (n === 36) return { ...c, word: 'USB', mode: 'usb', use: 'By custom, sideband calling on upper sideband.' };
  if (n > 36) return { ...c, word: 'USB', mode: 'usb', use: 'By custom, sideband stations use this one. An AM radio should stay off it.' };
  return c;
});
const CB_BOARD: Board = {
  id: 'cb',
  title: 'CB radio',
  strips: [
    { name: 'Emergency and highway', note: 'AM', keys: ['9', '19'] },
    { name: 'Everyday', note: 'AM', keys: [...keysOf(1, 8), ...keysOf(10, 18), ...keysOf(20, 35)] },
    { name: 'Sideband', note: 'by custom', keys: keysOf(36, 40) },
  ],
  channels: cbChannels,
  sides: [span(cbChannels)],
  reachHz: 5_000,
  home: hzOf(CB[18]),
  hint: 'Click a channel to tune it.',
};

// ── marine ──
// [channel, ship transmit, ship receive]; duplex channels are heard on both
const MARINE: [string, number, number][] = [
  ['1A', 156.05, 156.05], ['5A', 156.25, 156.25], ['6', 156.3, 156.3], ['7A', 156.35, 156.35], ['8', 156.4, 156.4],
  ['9', 156.45, 156.45], ['10', 156.5, 156.5], ['11', 156.55, 156.55], ['12', 156.6, 156.6], ['13', 156.65, 156.65],
  ['14', 156.7, 156.7], ['15', 156.75, 156.75], ['16', 156.8, 156.8], ['17', 156.85, 156.85], ['18A', 156.9, 156.9],
  ['19A', 156.95, 156.95], ['20', 157.0, 161.6], ['21A', 157.05, 157.05], ['22A', 157.1, 157.1], ['23A', 157.15, 157.15],
  ['24', 157.2, 161.8], ['25', 157.25, 161.85], ['26', 157.3, 161.9], ['27', 157.35, 161.95], ['28', 157.4, 162.0],
  ['63A', 156.175, 156.175], ['65A', 156.275, 156.275], ['66A', 156.325, 156.325], ['67', 156.375, 156.375],
  ['68', 156.425, 156.425], ['69', 156.475, 156.475], ['70', 156.525, 156.525], ['71', 156.575, 156.575],
  ['72', 156.625, 156.625], ['73', 156.675, 156.675], ['74', 156.725, 156.725], ['77', 156.875, 156.875],
  ['78A', 156.925, 156.925], ['79A', 156.975, 156.975], ['80A', 157.025, 157.025], ['81A', 157.075, 157.075],
  ['82A', 157.125, 157.125], ['83A', 157.175, 157.175], ['84', 157.225, 161.825], ['85', 157.275, 161.875],
  ['86', 157.325, 161.925], ['87', 157.375, 157.375], ['88A', 157.425, 157.425],
];
const MARINE_STRIPS: Strip[] = [
  { name: 'Distress and calling', keys: ['16', '9', '70'] },
  { name: 'Safety and Coast Guard', keys: ['6', '13', '67', '22A'] },
  { name: 'Boaters', note: 'noncommercial', keys: ['68', '69', '71', '72', '78A', '79A', '80A'] },
  { name: 'Commercial', keys: ['7A', '8', '10', '11', '18A', '19A', '88A'] },
  { name: 'Port operations', keys: ['1A', '5A', '12', '14', '20', '63A', '65A', '66A', '73', '74', '77'] },
  { name: 'Ship to shore', note: 'public shore stations', keys: ['24', '84', '25', '85', '26', '86', '27', '28'] },
  { name: 'Other', keys: ['15', '17', '21A', '23A', '81A', '82A', '83A', '87'] },
];
const MARINE_ABOUT: Record<string, { word?: string; use: string }> = {
  '16': { word: 'Distress', use: 'Distress, safety and calling. Boats must keep watch here.' },
  '9': { word: 'Calling', use: 'General calling for noncommercial boats.' },
  '70': { word: 'Digital', use: 'Digital distress and safety calls only. You hear data, not voice.' },
  '6': { word: 'Safety', use: 'Ship-to-ship safety.' },
  '13': { word: 'Bridge', use: 'Bridge-to-bridge navigation.' },
  '67': { word: 'Bridge', use: 'Navigation, commercial or noncommercial, depending on the area.' },
  '22A': { word: 'Coast Guard', use: 'Coast Guard broadcasts and talk with boats.' },
  '79A': { use: 'Commercial; noncommercial on the Great Lakes only.' },
  '80A': { use: 'Commercial; noncommercial on the Great Lakes only.' },
  '88A': { use: 'Limited use that depends on the area.' },
  '77': { word: 'Pilots', use: 'Talk with pilots about ship movement and docking.' },
  '20': { use: 'Port operations, with a ship side and a shore side.' },
  '15': { word: 'Environment', use: 'Environmental broadcasts from shore.' },
  '17': { word: 'Control', use: 'Maritime control, and government training.' },
  '87': { use: 'Set aside for ship tracking data (AIS) in the FCC table.' },
};
const MARINE_USE: Record<string, string> = {
  Boaters: 'Noncommercial boats.',
  Commercial: 'Commercial traffic.',
  'Port operations': 'Port operations.',
  'Ship to shore': 'Public shore stations talking with boats.',
  Other: 'Not in the FCC table for boats.',
};
const marineChannels = MARINE.flatMap(([ch, tx, rx]): BoardChannel[] => {
  const about = MARINE_ABOUT[ch] ?? { use: MARINE_USE[MARINE_STRIPS.find((s) => s.keys.includes(ch))!.name] };
  if (tx === rx) return [{ key: ch, hz: hzOf(tx), ...about }];
  return [
    { key: ch, hz: hzOf(rx), name: `${ch} shore`, twin: `${ch}S`, ...about },
    { key: `${ch}S`, hz: hzOf(tx), name: `${ch} ship`, twin: ch, second: true, ...about },
  ];
});
const MARINE_BOARD: Board = {
  id: 'marine',
  title: 'Marine',
  strips: MARINE_STRIPS,
  channels: marineChannels,
  sides: [span(marineChannels.filter((c) => c.hz < 160e6)), span(marineChannels.filter((c) => c.hz >= 160e6))],
  reachHz: 12_500,
  home: hzOf(156.8),
  halves: ['shore', 'ship'],
  secondName: 'Ship side',
  hint: 'Click a channel to tune it. Channels with two halves have a ship side and a shore side.',
};

// ── MURS ──
const MURS = [151.82, 151.88, 151.94, 154.57, 154.6];
const MURS_ABOUT: Record<string, { word?: string; use: string }> = {
  '4': { word: 'Blue Dot', use: 'Known as Blue Dot. Up to 20 kHz wide.' },
  '5': { word: 'Green Dot', use: 'Known as Green Dot. Up to 20 kHz wide.' },
};
const mursChannels = MURS.map((m, i): BoardChannel => ({ key: String(i + 1), hz: hzOf(m), ...(MURS_ABOUT[i + 1] ?? { use: 'Narrow FM only, 11.25 kHz.' }) }));
const MURS_BOARD: Board = {
  id: 'murs',
  title: 'MURS',
  strips: [
    { name: 'Narrow', note: '11.25 kHz', keys: ['1', '2', '3'] },
    { name: 'Wide or narrow', note: 'up to 20 kHz', keys: ['4', '5'] },
  ],
  channels: mursChannels,
  sides: [span(mursChannels.slice(0, 3)), span(mursChannels.slice(3))],
  reachHz: 7_500,
  hint: 'Click a channel to tune it.',
};

// ── NOAA ──
const wxChannels = [162.4, 162.425, 162.45, 162.475, 162.5, 162.525, 162.55].map((m, i): BoardChannel => ({
  key: String(i + 1),
  hz: hzOf(m),
  name: `WX${i + 1}`,
  use: 'NOAA Weather Radio. A nearby transmitter uses one of the seven.',
}));
const WX_BOARD: Board = {
  id: 'wx',
  title: 'NOAA weather',
  strips: [{ name: 'Weather broadcasts', note: 'always on', keys: keysOf(1, 7) }],
  channels: wxChannels,
  sides: [span(wxChannels)],
  reachHz: 12_500,
  hint: 'Click a channel to tune it.',
};

export const BOARDS = [WALKIE_BOARD, CB_BOARD, MARINE_BOARD, MURS_BOARD, WX_BOARD];
const BY_BAND: Record<string, Board> = Object.fromEntries(BOARDS.map((b) => [b.id, b]));
// bands files from before FRS and GMRS were one band
BY_BAND.frs_only = BY_BAND.gmrs = WALKIE_BOARD;

export const boardFor = (bandId: string | undefined): Board | undefined => (bandId ? BY_BAND[bandId] : undefined);
export const stripOf = (board: Board, key: string) => board.strips.find((s) => s.keys.includes(key));
export const channelByKey = (board: Board, key: string) => board.channels.find((c) => c.key === key);

/** A tile's strip for a channel, following a second channel to its twin. */
export function stripOfChannel(board: Board, c: BoardChannel): Strip | undefined {
  return stripOf(board, c.second && c.twin ? c.twin : c.key);
}

// room either side of a side's channels when the view zooms to it
const VIEW_MARGIN_HZ = 25_000;

/** Index of the side nearest `hz`. */
export function sideOf(board: Board, hz: number): number {
  let best = 0;
  const off = ([lo, hi]: [number, number]) => (hz < lo ? lo - hz : hz > hi ? hz - hi : 0);
  board.sides.forEach((s, i) => {
    if (off(s) < off(board.sides[best])) best = i;
  });
  return best;
}

/** The view that shows every channel on the side holding `hz`. */
export function sideView(board: Board, hz: number): [number, number] {
  const [lo, hi] = board.sides[sideOf(board, hz)];
  return [lo - VIEW_MARGIN_HZ, hi + VIEW_MARGIN_HZ];
}

/** The sides whose every channel lies between `lo` and `hi`. */
export function sidesIn(board: Board, lo: number, hi: number): number[] {
  return board.sides.flatMap(([a, b], i) => (a - board.reachHz >= lo && b + board.reachHz <= hi ? [i] : []));
}

/** The channel a signal at `hz` is on, if any. */
export function channelOn(board: Board, hz: number): BoardChannel | undefined {
  let best: BoardChannel | undefined;
  for (const c of board.channels) if (!best || Math.abs(c.hz - hz) < Math.abs(best.hz - hz)) best = c;
  return best && Math.abs(best.hz - hz) <= board.reachHz ? best : undefined;
}

/** Strongest signal per channel key, dB above the floor. */
export function liveOn(board: Board, signals: { hz: number; snr_db?: number }[]): Map<string, number> {
  const out = new Map<string, number>();
  for (const s of signals) {
    const c = channelOn(board, s.hz);
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
  /** Sides listened to during the minute, one bit per side: side i is bit 1 << i. */
  sides: number;
  /** Channel keys heard during the minute. */
  on: string[];
}

/**
 * Adds what is heard now to the minute holding `ms`, and drops minutes older than KEEP_MIN. Returns true when this
 * started a new minute, so the finished one can be saved.
 */
export function record(board: Board, hist: Minute[], ms: number, sides: number[], live: Iterable<string>): boolean {
  const t = Math.floor(ms / 60_000);
  let m = hist.at(-1);
  const started = !m || m.t !== t;
  if (started) {
    m = { t, sides: 0, on: [] };
    hist.push(m);
    while (hist.length && hist[0].t <= t - KEEP_MIN) hist.shift();
  }
  for (const s of sides) m!.sides |= 1 << s;
  for (const k of live) {
    const c = channelByKey(board, k);
    if (c && m!.sides & (1 << sideOf(board, c.hz)) && !m!.on.includes(k)) m!.on.push(k);
  }
  return started;
}

export function parseHistory(list: unknown, nowMs: number): Minute[] {
  const t = Math.floor(nowMs / 60_000);
  if (!Array.isArray(list)) return [];
  return list.filter((m): m is Minute => typeof m?.t === 'number' && typeof m.sides === 'number' && Array.isArray(m.on) && m.t > t - KEEP_MIN && m.t <= t);
}

/** Minutes in the window during which any side was listened to. */
export const listened = (hist: Minute[], nowMs: number) => hist.filter((m) => m.sides && m.t > Math.floor(nowMs / 60_000) - KEEP_MIN).length;

export interface ActivityRow {
  channel: BoardChannel;
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
export function activity(board: Board, hist: Minute[], nowMs: number): Activity {
  const now = Math.floor(nowMs / 60_000);
  const first = hist.find((m) => m.sides && m.t > now - KEEP_MIN)?.t ?? now;
  const spanMin = Math.min(KEEP_MIN, Math.max(SHOW_AFTER_MIN, now - first + 1));
  const cols = Math.min(COLUMNS, spanMin);
  const start = now - spanMin + 1;
  const col = (t: number) => Math.min(cols - 1, Math.floor(((t - start) * cols) / spanMin));
  const rows = board.channels.map((channel) => {
    const bit = 1 << sideOf(board, channel.hz);
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
