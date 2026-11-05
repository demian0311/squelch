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
