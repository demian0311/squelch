// EMS band (P25 trunked radio): what the panel says about the receiver and each call.
import type { P25, P25Talkgroup } from './api';
import { fmtMHz } from './radio';

/** The sub-line under the band name while the radio hops across the band: where it is and how far along. */
export function scanSay(s: NonNullable<P25['scan']>): string {
  return `Scanning for a control channel · hop ${s.hop} of ${s.hops} · ${fmtMHz(s.lo_hz)}–${fmtMHz(s.hi_hz)} MHz`;
}

/** One line for the folded panel. */
export function p25Summary(p: P25): string {
  if (p.call) return `Playing talkgroup ${p.call.group}`;
  if (p.state === 'searching') return p.scan ? scanSay(p.scan) : 'Looking for a control channel in this capture…';
  const site = p25Site(p);
  return site ? `${site} · waiting for a call` : 'Control channel found · reading the site';
}

/** "Site 3-55 · system D1C", once the control channel has said so. */
export function p25Site(p: P25): string | null {
  if (p.rfss === null || p.site === null) return null;
  const sys = p.system !== null ? ` · system ${p.system.toString(16).toUpperCase()}` : '';
  return `Site ${p.rfss}-${p.site}${sys}`;
}

/** The glyph for a call this receiver will never play, with its tooltip; null when it can play. */
/** Mirrors the engine's `p25::WINDOW_MIN`: the minutes a talkgroup's activity bars cover. */
export const WINDOW_MIN = 10;

/** What the user picked on a system: the one talkgroup that plays, and those that never do. */
export interface Picks {
  hold: number | null;
  skip: number[];
}
export const NO_PICKS: Picks = { hold: null, skip: [] };

/** Where picks are remembered: the system id, which every site of a system shares. Null before it is read. */
export function systemKey(p: P25 | null): string | null {
  return p?.system == null ? null : p.system.toString(16);
}

/** The talkgroups this receiver can play: the held one first, then busiest first, skipped ones last. */
export function groupRows(groups: P25Talkgroup[], picks: Picks): P25Talkgroup[] {
  const rank = (g: P25Talkgroup) => (picks.hold === g.group ? 0 : picks.skip.includes(g.group) ? 2 : 1);
  return groups.filter((g) => !g.encrypted && !g.tdma).sort((a, b) => rank(a) - rank(b) || b.calls - a.calls || b.last_secs - a.last_secs);
}

/** One phrase each for the calls this receiver will never play, null when there were none. */
export function folded(groups: P25Talkgroup[]): { encrypted: string | null; tdma: string | null } {
  const say = (gs: P25Talkgroup[], what: string) => {
    if (!gs.length) return null;
    const calls = gs.reduce((n, g) => n + g.calls, 0);
    return `${calls} ${what} ${calls === 1 ? 'call' : 'calls'} on ${gs.length} ${gs.length === 1 ? 'talkgroup' : 'talkgroups'}`;
  };
  return { encrypted: say(groups.filter((g) => g.encrypted), 'encrypted'), tdma: say(groups.filter((g) => !g.encrypted && g.tdma), 'Phase 2') };
}

/** Height of one activity bar: a stub for a quiet minute, else scaled to the busiest minute of the row. */
export function barPx(calls: number, max: number, tall = 14): number {
  return calls ? 3 + Math.round((tall - 3) * (calls / Math.max(1, max))) : 2;
}

/** The sub-line while a hold keeps the receiver quiet: what it holds and when that talkgroup last spoke. */
export function holdSay(p: P25, hold: number): string {
  const g = p.groups.find((g) => g.group === hold);
  return `Holding talkgroup ${hold} · ${g ? `last call ${sinceSay(p.now_secs - g.last_secs)}` : 'no call yet'}`;
}

export function sinceSay(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  return s < 60 ? `${s} s ago` : `${Math.round(s / 60)} min ago`;
}

/** "12 s ago", "3 min ago". */

// ── the EMS screen: where the receiver is, what to make of it, the tower as a board ──

/** A locked tower with no call for this long is called quiet. Measured 2026-11-04 in Miami: busy towers gave a call every 4–5 s. */
export const QUIET_SECS = 120;
/** A channel stays lit this long after its last grant: about one call. */
export const LIT_SECS = 6;

export interface Step {
  n: 1 | 2 | 3;
  say: string;
  state: 'done' | 'now' | 'todo';
}

/** The three steps the receiver always takes, each with where it stands. */
export function steps(p: P25 | null, lockSecs: number): Step[] {
  const locked = !!p && p.state !== 'searching' && p.control_hz !== null;
  const site = p ? p25Site(p) : null;
  const s1: Step = locked ? { n: 1, say: `Found a control channel · ${fmtMHz(p!.control_hz!)} MHz`, state: 'done' } : { n: 1, say: p ? p25Summary(p) : 'Find a control channel', state: 'now' };
  const s2: Step = !locked ? { n: 2, say: 'Read the tower', state: 'todo' } : site ? { n: 2, say: `Read the tower · ${site.replace(/^Site /, 'site ').replace(' · system ', ' of system ')}`, state: 'done' } : { n: 2, say: 'Reading the tower…', state: 'now' };
  let s3: Step = { n: 3, say: 'Hear calls', state: 'todo' };
  if (locked && site) {
    const playable = p!.groups.filter((g) => !g.encrypted && !g.tdma);
    const last = playable.reduce<number | null>((m, g) => (m === null || g.last_secs > m ? g.last_secs : m), null);
    s3 = p!.call ? { n: 3, say: `Hearing calls · talkgroup ${p!.call.group} playing`, state: 'now' } : last !== null ? { n: 3, say: `Hearing calls · last one ${sinceSay(p!.now_secs - last)}`, state: 'now' } : { n: 3, say: `Hearing calls · none for ${sinceSay(lockSecs).replace(' ago', '')}`, state: 'now' };
  }
  return [s1, s2, s3];
}

export interface Verdict {
  say: string;
  /** Something worth doing about it, or null: wait. */
  action: 'other-band' | null;
}

/** One sentence on whether this tower is worth waiting on, from the numbers the receiver has. `lockSecs`: how long it has sat on it. */
export function verdict(p: P25 | null, picks: Picks, lockSecs: number): Verdict {
  if (!p || p.state === 'searching') return { say: 'Every carrier that stays on is tested for a control channel. A tower is usually found within a minute.', action: null };
  if (!p25Site(p)) return { say: 'Control channel found. The tower names itself within a few seconds.', action: null };
  if (p.call) return { say: `Playing talkgroup ${p.call.group} on ${fmtMHz(p.call.hz)} MHz.`, action: null };
  if (picks.hold !== null) return { say: `${holdSay(p, picks.hold)}. Click the held talkgroup to let every talkgroup play.`, action: null };
  const span = Math.max(1, Math.min(lockSecs, WINDOW_MIN * 60));
  const playable = p.groups.filter((g) => !g.encrypted && !g.tdma).reduce((n, g) => n + g.calls, 0);
  const locked = p.groups.reduce((n, g) => n + g.calls, 0) - playable;
  const every = (n: number) => `every ${Math.max(1, Math.round(span / n))} s`;
  const rate = `${p.messages_per_sec.toFixed(0)} messages a second`;
  if (playable) return { say: `A call you can hear ${every(playable)} on this tower. Waiting for the next one.`, action: null };
  if (locked) return { say: `Calls ${every(locked)} here, all encrypted or Phase 2: nothing this receiver can play on this tower.`, action: 'other-band' };
  if (lockSecs < QUIET_SECS) return { say: `Reading the tower: ${rate}. Calls usually come within a minute.`, action: null };
  return { say: `This tower is quiet: ${rate} but no call in ${sinceSay(lockSecs).replace(' ago', '')}.`, action: 'other-band' };
}

export interface Channel {
  hz: number;
  kind: 'control' | 'voice';
  /** The talkgroup last granted this channel. */
  group: number | null;
  /** A call was granted on it within `LIT_SECS`. */
  lit: boolean;
  playing: boolean;
  /** The last call on it could play. */
  playable: boolean;
  encrypted: boolean;
}

/** The tower as a board: its control channel, then every voice channel a call was granted on, by frequency. */
export function channelBoard(p: P25): Channel[] {
  const out: Channel[] = [];
  if (p.control_hz !== null) out.push({ hz: p.control_hz, kind: 'control', group: null, lit: true, playing: false, playable: false, encrypted: false });
  const voice = new Map<number, P25Talkgroup>();
  for (const g of p.groups) {
    const had = voice.get(g.hz);
    if (!had || g.last_secs > had.last_secs) voice.set(g.hz, g);
  }
  const rows = [...voice.values()].sort((a, b) => a.hz - b.hz).map((g): Channel => ({
    hz: g.hz,
    kind: 'voice',
    group: g.group,
    lit: p.now_secs - g.last_secs < LIT_SECS || p.call?.hz === g.hz,
    playing: p.call?.hz === g.hz,
    playable: !g.encrypted && !g.tdma,
    encrypted: g.encrypted,
  }));
  return out.concat(rows);
}

/** Calls per minute across the window, oldest first: those this receiver can play and those it cannot. */
export function minuteBars(groups: P25Talkgroup[]): { playable: number; locked: number }[] {
  const bars = Array.from({ length: WINDOW_MIN }, () => ({ playable: 0, locked: 0 }));
  for (const g of groups) {
    const can = !g.encrypted && !g.tdma;
    g.minutes.forEach((n, i) => {
      if (i < WINDOW_MIN) bars[i][can ? 'playable' : 'locked'] += n;
    });
  }
  return bars;
}
