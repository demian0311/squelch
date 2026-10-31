// Police and fire (P25 trunked radio): what the panel says about the receiver and each call.
import type { P25, P25Call } from './api';

/** One line for the folded panel. */
export function p25Summary(p: P25): string {
  if (p.call) return `Playing talkgroup ${p.call.group}`;
  if (p.state === 'searching') return 'Looking for a control channel…';
  const site = p25Site(p);
  return site ? `${site} · waiting for a call` : 'Control channel found · reading the site';
}

/** "Site 3-55 · system D1C", once the control channel has said so. */
export function p25Site(p: P25): string | null {
  if (p.rfss === null || p.site === null) return null;
  const sys = p.system !== null ? ` · system ${p.system.toString(16).toUpperCase()}` : '';
  return `Site ${p.rfss}-${p.site}${sys}`;
}

/** Why a call is not playing, or null when it can. */
export function callBlock(c: P25Call, voiceError: string | null): string | null {
  if (c.encrypted) return 'Encrypted';
  if (c.tdma) return 'Phase 2, not supported yet';
  return voiceError ? 'Voice decoder missing' : null;
}

/** "12 s ago", "3 min ago". */
export function callAge(c: P25Call, nowSecs: number): string {
  const s = Math.max(0, Math.round(nowSecs - c.at_secs));
  return s < 60 ? `${s} s ago` : `${Math.round(s / 60)} min ago`;
}
