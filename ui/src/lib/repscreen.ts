// The Repeaters screen: every repeater in reach as a row, FM or DMR, what each is doing now, and
// which one plays. The engine's `RepeaterScan` hears every row inside the capture at once.
import type { RepeaterScan, RepeaterState } from './api';
import { callSay, lengthSay } from './dmr';
import { sinceSay } from './p25';
import { PLAYS, undecoded, type RepeaterPoint } from './repeaters';

/** A repeater's last activity is remembered this long across runs (`repeaters.heard` in state.json). */
export const REMEMBER_MS = 7 * 24 * 3600_000;
/** The memory is rewritten at most this often per repeater while it stays active. */
const REMEMBER_EVERY_MS = 5_000;
/** A channel this close to the capture's edge or its centre spike is not heard well. */
const EDGE_HZ = 30_000;
const DC_HZ = 20_000;

export interface Channel {
  hz: number;
  dmr: boolean;
}

/** The outputs the engine watches: every repeater the radio can play, one channel per frequency. */
export function scanChannels(points: RepeaterPoint[]): Channel[] {
  const out: Channel[] = [];
  for (const p of points) {
    if (undecoded(p) || out.some((c) => c.hz === p.hz)) continue;
    out.push({ hz: p.hz, dmr: p.digital === PLAYS });
  }
  return out;
}

/**
 * Where the capture sits: the centre that hears the most repeaters, DMR ones first, those in reach
 * before those probably too far. `must` (a clicked row) has to be inside. The centre avoids landing
 * on a channel, where the dongle's own spike sits.
 */
export function bestCentre(points: RepeaterPoint[], rate: number, must: number | null = null): number | null {
  const half = rate * 0.45 - EDGE_HZ;
  const chans = scanChannels(points);
  if (!chans.length) return must;
  const worth = (hz: number) => {
    const p = points.find((p) => p.hz === hz)!;
    return (p.digital === PLAYS ? 100 : 1) * (p.reach ? 10 : 1);
  };
  const score = (c: number) => chans.reduce((n, ch) => n + (Math.abs(ch.hz - c) <= half ? worth(ch.hz) : 0), 0);
  let candidates = chans.flatMap((ch) => [ch.hz - half, ch.hz + half, ch.hz + DC_HZ * 2]);
  if (must !== null) candidates = candidates.filter((c) => Math.abs(must - c) <= half);
  if (!candidates.length) return must;
  let best = candidates[0];
  let bestScore = -1;
  for (const c of candidates) {
    const s = score(c);
    if (s > bestScore) [best, bestScore] = [c, s];
  }
  // off any channel's own frequency
  while (chans.some((ch) => Math.abs(ch.hz - best) < DC_HZ)) best += DC_HZ;
  return Math.round(best);
}

export type RowState = 'playing' | 'talking' | 'under' | 'quiet' | 'away';

/** The facts a row shows as a glyph and a number; the sentence stays in `detail` for the sub-line and tooltips. */
export interface RepFacts {
  /** Seconds since the repeater last carried something; null when nothing in `REMEMBER_MS`. */
  lastSecs: number | null;
  /** The DMR call's destination, "Talkgroup 31088" or "Radio 3104321, private"; null without a call. */
  to: string | null;
  /** How long the call has run, "42 s" or "1:05". */
  length: string | null;
  /** Which slots carry a call, and whether that call is encrypted. */
  slots: { call: boolean; encrypted: boolean }[];
}

export interface RepRow {
  p: RepeaterPoint;
  dmr: boolean;
  /** Inside the capture, heard now. */
  heard: boolean;
  held: boolean;
  state: RowState;
  /** The word: "Talking · playing", "On air", "Quiet", "Out of the capture". */
  say: string;
  /** The small line under it. */
  detail: string;
  /** The strongest carrier on the output now, dB above the floor; null when none is detected or the row is out of the capture. */
  snr: number | null;
  facts: RepFacts;
}

/** A row's live state from the engine. */
const stateOf = (scan: RepeaterScan | null, hz: number): RepeaterState | null => scan?.channels.find((c) => Math.abs(c.hz - hz) < 1) ?? null;

/** The call a DMR repeater carries now: the playing slot's, else the first. */
function dmrCall(st: RepeaterState) {
  const slots = st.dmr?.slots ?? [];
  return slots.find((s) => s.call && s.playing)?.call ?? slots.find((s) => s.call)?.call ?? null;
}

/** "last call 6 min ago" from the memory, or how long the memory reaches. */
function lastSay(dmr: boolean, remembered: number | undefined, nowMs: number): string {
  const what = dmr ? 'last call' : 'last heard';
  return remembered === undefined ? `nothing heard in ${Math.round(REMEMBER_MS / 86_400_000)} days` : `${what} ${sinceSay((nowMs - remembered) / 1000)}`;
}

/**
 * `levels`: the strongest detected carrier per output (`liveOutputs` with no squelch), so a carrier under
 * the mark shows as `under` with its level, where the engine's `on_air` (judged against the squelch) says quiet.
 */
/** "40 s", "6 min", "2 h", "3 d": the number beside the clock glyph. */
export function shortAgo(secs: number): string {
  if (secs < 60) return `${Math.round(secs)} s`;
  if (secs < 3600) return `${Math.floor(secs / 60)} min`;
  if (secs < 86_400) return `${Math.floor(secs / 3600)} h`;
  return `${Math.floor(secs / 86_400)} d`;
}

export function repRows(points: RepeaterPoint[], scan: RepeaterScan | null, memory: Record<string, number>, nowMs: number, levels: Map<number, number> = new Map()): RepRow[] {
  return points
    .filter((p) => !undecoded(p))
    .map((p) => {
      const dmr = p.digital === PLAYS;
      const st = stateOf(scan, p.hz);
      const held = scan?.hold !== null && scan?.hold !== undefined && Math.abs(scan.hold - p.hz) < 1;
      const playing = scan?.playing !== null && scan?.playing !== undefined && Math.abs(scan.playing - p.hz) < 1;
      const remembered = memory[String(p.hz)];
      const lastSecs = remembered === undefined ? null : Math.max(0, (nowMs - remembered) / 1000);
      const slots = (st?.dmr?.slots ?? []).map((s) => ({ call: !!s.call, encrypted: !!s.call?.encrypted }));
      const quietFacts: RepFacts = { lastSecs, to: null, length: null, slots };
      if (!st || !st.heard) {
        return { p, dmr, heard: false, held, state: 'away' as const, say: 'Out of the capture', detail: `click to move there · ${lastSay(dmr, remembered, nowMs)}`, snr: null, facts: quietFacts };
      }
      const snr = levels.get(p.hz) ?? null;
      const call = dmr ? dmrCall(st) : null;
      if (call) {
        const say = call.encrypted ? 'Talking · encrypted, muted' : playing ? 'Talking · playing' : 'Talking';
        const slot = st.dmr!.slots.filter((s) => s.call).length === 2 ? ' · both slots busy' : ` · slot ${call.slot}`;
        const facts: RepFacts = { lastSecs: 0, to: callSay(call), length: lengthSay(call), slots };
        return { p, dmr, heard: true, held, state: playing ? ('playing' as const) : ('talking' as const), say, detail: `${callSay(call)} · ${lengthSay(call)}${slot}`, snr, facts };
      }
      if (!dmr && st.on_air) {
        return { p, dmr, heard: true, held, state: playing ? ('playing' as const) : ('talking' as const), say: playing ? 'On air · playing' : 'On air', detail: playing ? 'a carrier now' : 'a carrier now · click to hear it', snr, facts: { ...quietFacts, lastSecs: 0 } };
      }
      if (!dmr && snr !== null) {
        return { p, dmr, heard: true, held, state: 'under' as const, say: 'Carrier under the mark', detail: `${snr.toFixed(0)} dB, muted · ${lastSay(dmr, remembered, nowMs)}`, snr, facts: quietFacts };
      }
      return { p, dmr, heard: true, held, state: 'quiet' as const, say: playing ? 'Quiet · ready to play' : 'Quiet', detail: lastSay(dmr, remembered, nowMs), snr, facts: quietFacts };
    });
}

/** Per output frequency, the wall-clock time it last carried something; `null` when nothing changed. */
export function rememberHeard(memory: Record<string, number>, scan: RepeaterScan | null, nowMs: number): Record<string, number> | null {
  let next: Record<string, number> | null = null;
  for (const c of scan?.channels ?? []) {
    if (c.last_on_secs === null || scan!.now_secs - c.last_on_secs > 2) continue;
    const key = String(c.hz);
    if (nowMs - (memory[key] ?? 0) < REMEMBER_EVERY_MS) continue;
    next ??= { ...memory };
    next[key] = nowMs;
  }
  for (const [key, at] of Object.entries(next ?? memory)) {
    if (nowMs - at > REMEMBER_MS) {
      next ??= { ...memory };
      delete next[key];
    }
  }
  return next;
}
