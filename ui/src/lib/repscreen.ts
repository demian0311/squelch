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

export type RowState = 'playing' | 'talking' | 'quiet' | 'away';

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

export function repRows(points: RepeaterPoint[], scan: RepeaterScan | null, memory: Record<string, number>, nowMs: number): RepRow[] {
  return points
    .filter((p) => !undecoded(p))
    .map((p) => {
      const dmr = p.digital === PLAYS;
      const st = stateOf(scan, p.hz);
      const held = scan?.hold !== null && scan?.hold !== undefined && Math.abs(scan.hold - p.hz) < 1;
      const playing = scan?.playing !== null && scan?.playing !== undefined && Math.abs(scan.playing - p.hz) < 1;
      const remembered = memory[String(p.hz)];
      if (!st || !st.heard) {
        return { p, dmr, heard: false, held, state: 'away' as const, say: 'Out of the capture', detail: `click to move there · ${lastSay(dmr, remembered, nowMs)}` };
      }
      const call = dmr ? dmrCall(st) : null;
      if (call) {
        const say = call.encrypted ? 'Talking · encrypted, muted' : playing ? 'Talking · playing' : 'Talking';
        const slot = st.dmr!.slots.filter((s) => s.call).length === 2 ? ' · both slots busy' : ` · slot ${call.slot}`;
        return { p, dmr, heard: true, held, state: playing ? ('playing' as const) : ('talking' as const), say, detail: `${callSay(call)} · ${lengthSay(call)}${slot}` };
      }
      if (!dmr && st.on_air) {
        return { p, dmr, heard: true, held, state: playing ? ('playing' as const) : ('talking' as const), say: playing ? 'On air · playing' : 'On air', detail: playing ? 'a carrier now' : 'a carrier now · click to hear it' };
      }
      return { p, dmr, heard: true, held, state: 'quiet' as const, say: playing ? 'Quiet · ready to play' : 'Quiet', detail: lastSay(dmr, remembered, nowMs) };
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

export interface RepStep {
  n: 1 | 2 | 3;
  say: string;
  state: 'done' | 'now' | 'todo';
}

export function repSteps(rows: RepRow[]): RepStep[] {
  const near = rows.filter((r) => r.p.reach).length;
  const heard = rows.filter((r) => r.heard).length;
  const playing = rows.find((r) => r.state === 'playing');
  const held = rows.find((r) => r.held);
  const s1: RepStep = near ? { n: 1, say: `${near} ${near === 1 ? 'repeater' : 'repeaters'} in reach`, state: 'done' } : { n: 1, say: 'No repeater in reach', state: 'now' };
  const s2: RepStep = heard ? { n: 2, say: `${heard} in the capture, heard at once`, state: 'done' } : { n: 2, say: 'none in the capture', state: near ? 'now' : 'todo' };
  const s3: RepStep = playing ? { n: 3, say: `playing ${playing.p.call}`, state: 'now' } : held ? { n: 3, say: `holding ${held.p.call}`, state: 'now' } : { n: 3, say: 'waiting for someone to talk', state: heard ? 'now' : 'todo' };
  return [s1, s2, s3];
}

/** One sentence on what the radio is doing and what a click does. */
export function repVerdict(rows: RepRow[], memory: Record<string, number>, nowMs: number): string {
  const heard = rows.filter((r) => r.heard);
  if (!rows.length) return 'No repeater this radio can play is listed near you on this band.';
  if (!heard.length) return 'No repeater is inside the capture. Click a row to move the radio onto it.';
  const held = rows.find((r) => r.held);
  const playing = rows.find((r) => r.state === 'playing');
  const quiet = heard.filter((r) => r.state === 'quiet');
  const recent = quiet.map((r) => ({ r, at: memory[String(r.p.hz)] })).filter((x) => x.at !== undefined).sort((a, b) => b.at! - a.at!)[0];
  const lastLine = recent ? ` ${recent.r.p.call} was last heard ${sinceSay((nowMs - recent.at!) / 1000)}.` : '';
  if (held) {
    const doing = playing && playing === held ? (held.dmr ? 'carries a call' : 'is on air') : 'is quiet';
    return `Holding ${held.p.call} in ${held.p.city}: only it plays, and it ${doing}. Click it again to let the radio play whoever talks.`;
  }
  if (playing) {
    const others = heard.length - 1;
    const quietLine = others ? ` ${others === 1 ? 'One other repeater sits' : `${others} other repeaters sit`} in this capture and ${others === 1 ? 'is' : 'are'} quiet.` : '';
    return `${playing.dmr ? 'Someone is talking' : 'A carrier is on'} ${playing.p.call} in ${playing.p.city}.${quietLine}${lastLine} Click a row to stay on it; otherwise the radio plays whoever talks next.`;
  }
  return `Quiet. ${heard.length === 1 ? 'One repeater is' : `${heard.length} repeaters are`} heard at once; the first to carry a call plays.${lastLine}`;
}
