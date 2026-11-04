// EMS band (P25 trunked radio): what the panel says about the receiver and each call.
import type { P25, P25Call } from './api';
import { fmtMHz } from './radio';

/** The top bar's chip while the radio hops across the band: where it is and how far along. */
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
export function callGlyph(c: P25Call): { icon: 'lock' | 'mute'; say: string } | null {
  if (c.encrypted) return { icon: 'lock', say: 'Encrypted: no scanner can hear it' };
  if (c.tdma) return { icon: 'mute', say: 'Phase 2: two calls share one channel in turns, which this receiver cannot play yet' };
  return null;
}

/** "12 s ago", "3 min ago". */
export function callAge(c: P25Call, nowSecs: number): string {
  const s = Math.max(0, Math.round(nowSecs - c.at_secs));
  return s < 60 ? `${s} s ago` : `${Math.round(s / 60)} min ago`;
}
