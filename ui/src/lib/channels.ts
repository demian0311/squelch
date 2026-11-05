// Channel plans for bands where radios pick numbered channels, keyed by band id. Bands with tiles get theirs from board.ts.
// Sources (checked 2026-10-30): railroad AAR 007–097 at 15 kHz from 160.215.
// Ham bands get the user's repeaters file instead (see setRepeaters).

import type { Band, Mode, Repeater } from './api';
import { BOARDS, stripOfChannel, type Board, type BoardChannel } from './board';

export interface Channel {
  hz: number;
  name: string;
  /** What the channel is for, shown on its card. */
  group?: string;
  /** Tuning it switches to this mode, whatever the band's own. */
  mode?: Mode;
}

const mhz = (m: number) => Math.round(m * 1e6);

// How each band's channels are named on the waterfall.
const CARD: Record<string, (c: BoardChannel) => string> = {
  walkie: (c) => `Channel ${c.key}`,
  cb: (c) => `CB ${c.key}`,
  marine: (c) => `Marine ${c.name ?? c.key}`,
  murs: (c) => `MURS ${c.key}`,
  wx: (c) => c.name!,
};
const planOf = (b: Board): Channel[] =>
  b.channels.map((c) => ({
    hz: c.hz,
    name: CARD[b.id](c),
    group: c.second && b.secondName && !b.halves ? 'Repeater inputs' : (c.word ?? stripOfChannel(b, c)?.name),
    ...(c.mode ? { mode: c.mode } : {}),
  }));

export const CHANNELS: Record<string, Channel[]> = {
  ...Object.fromEntries(BOARDS.map((b) => [b.id, planOf(b)])),
  rail: Array.from({ length: 91 }, (_, i) => ({ hz: mhz(160.215) + i * 15_000, name: `AAR ${String(i + 7).padStart(3, '0')}` })),
};
// bands files from before FRS and GMRS were one band
CHANNELS.frs_only = CHANNELS.gmrs = CHANNELS.walkie;

// repeater outputs by band id, for bands without a fixed plan
let repeaterPlans: Record<string, Channel[]> = {};
// a lone repeater in a band would otherwise claim every signal in it
const REPEATER_REACH_HZ = 5_000;
// standard US input offsets; 6 m varies too much to name
const SHIFTS: Record<string, string> = { '2m': '600 kHz', '125': '1.6 MHz', '70cm': '5 MHz' };

function repeaterNote(r: Repeater, bandId: string): string {
  const shift = r.shift === 's' ? 'split' : r.shift ? `${r.shift === '-' ? '−' : '+'}${SHIFTS[bandId] ?? ' offset'}` : null;
  const tone = /^\d/.test(r.tone) ? `${r.tone} Hz` : r.tone.startsWith('D') ? `DCS ${r.tone.slice(1)}` : r.tone === 'CSQ' ? 'no tone' : null;
  return [r.city, shift, tone, ...r.modes].filter(Boolean).join(' · ');
}

/** Use these repeaters as the channel plan of whichever band holds each, unless it has a fixed plan. */
export function setRepeaters(list: Repeater[], bands: Band[]) {
  const plans: Record<string, Channel[]> = {};
  for (const r of list) {
    const b = bands.find((b) => r.mhz >= b.low_mhz && r.mhz <= b.high_mhz);
    if (!b || CHANNELS[b.id]) continue;
    // repeaters are FM even on bands whose default is sideband, like 6 m; one listed under DMR
    // is nearly always digital only, so it opens in DMR (the panel offers FM)
    (plans[b.id] ??= []).push({ hz: mhz(r.mhz), name: r.call, group: repeaterNote(r, b.id), mode: r.modes.includes('DMR') ? 'dmr' : 'nfm' });
  }
  for (const p of Object.values(plans)) steps.set(p, Math.min(halfStep(p), REPEATER_REACH_HZ));
  repeaterPlans = plans;
}

const planFor = (bandId: string | undefined): Channel[] | undefined => (bandId ? CHANNELS[bandId] ?? repeaterPlans[bandId] : undefined);

/** The channel nearest `hz` in a band's plan, if within half the plan's tightest spacing. */
export function channelAt(bandId: string | undefined, hz: number): Channel | undefined {
  const plan = planFor(bandId);
  if (!plan) return undefined;
  let best: Channel | undefined;
  for (const c of plan) if (!best || Math.abs(c.hz - hz) < Math.abs(best.hz - hz)) best = c;
  return best && Math.abs(best.hz - hz) <= halfStep(plan) ? best : undefined;
}

export interface ChannelSection {
  /** The channels' shared purpose, or null for a plan without groups. */
  group: string | null;
  /** `snr` is the strongest signal on the channel, dB above the floor, when known. */
  channels: { channel: Channel; live: boolean; snr: number | null }[];
}

/** A fixed plan as a list to pick from, in plan order, each channel with the signal on it if any. */
export function channelList(bandId: string | undefined, signals: { hz: number; snr_db?: number }[]): ChannelSection[] {
  const plan = bandId ? CHANNELS[bandId] : undefined;
  if (!plan) return [];
  const live = new Map<number, number>();
  for (const s of signals) {
    const hz = channelAt(bandId, s.hz)?.hz;
    if (hz !== undefined) live.set(hz, Math.max(live.get(hz) ?? -Infinity, s.snr_db ?? -Infinity));
  }
  const out: ChannelSection[] = [];
  for (const c of plan) {
    const group = c.group ?? null;
    if (out.at(-1)?.group !== group || !out.length) out.push({ group, channels: [] });
    const snr = live.get(c.hz);
    out.at(-1)!.channels.push({ channel: c, live: snr !== undefined, snr: snr !== undefined && snr > -Infinity ? snr : null });
  }
  return out;
}

/** Channels between `vs` and `ve` with no signal on them, in plan order. */
export function quietChannels(bandId: string | undefined, vs: number, ve: number, signals: { hz: number }[]): Channel[] {
  const plan = planFor(bandId);
  if (!plan) return [];
  const busy = new Set(signals.map((s) => channelAt(bandId, s.hz)?.hz));
  return plan.filter((c) => c.hz >= vs && c.hz <= ve && !busy.has(c.hz));
}

/** Minimum gap between channel numbers on the frequency scale, in pixels. */
export const CHANNEL_LABEL_PX = 30;

export interface ChannelTick {
  left: number;
  text: string | null;
}

/**
 * Channel marks for the frequency scale between `vs` and `ve`, as % from the left. Every
 * channel gets a mark; numbers are thinned so they sit at least CHANNEL_LABEL_PX apart.
 */
export function channelTicks(bandId: string | undefined, vs: number, ve: number, widthPx: number): ChannelTick[] {
  const plan = planFor(bandId);
  if (!plan || widthPx <= 0) return [];
  const span = ve - vs;
  const inView = plan.filter((c) => c.hz > vs && c.hz < ve).sort((a, b) => a.hz - b.hz);
  let lastPx = -Infinity;
  return inView.map((c) => {
    const px = ((c.hz - vs) / span) * widthPx;
    const fits = px - lastPx >= CHANNEL_LABEL_PX && px > 12 && px < widthPx - 12;
    if (fits) lastPx = px;
    return { left: (px / widthPx) * 100, text: fits ? c.name.replace(/^\S+ /, '').replace(/ (ship|shore)$/, '') : null };
  });
}

const steps = new WeakMap<Channel[], number>();
function halfStep(plan: Channel[]): number {
  let s = steps.get(plan);
  if (s === undefined) {
    const hz = plan.map((c) => c.hz).sort((a, b) => a - b);
    s = Infinity;
    for (let i = 1; i < hz.length; i++) if (hz[i] > hz[i - 1]) s = Math.min(s, hz[i] - hz[i - 1]);
    s /= 2;
    steps.set(plan, s);
  }
  return s;
}
