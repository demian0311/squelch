// ACARS: the panel's list of aircraft messages.
import type { Acars, AcarsMessage } from './api';

/** Messages the panel keeps; each status carries only the last few seconds' worth. */
export const MAX_MESSAGES = 200;

/** `list` (newest first) plus whatever in `incoming` it lacks; the same array when nothing is new, so nothing redraws.
 * Ids count up from 1 each time decoding starts, so a message is known by its id and its time together. */
export function mergeMessages(list: AcarsMessage[], incoming: AcarsMessage[], max = MAX_MESSAGES): AcarsMessage[] {
  const fresh = incoming.filter((m) => !list.some((o) => o.id === m.id && o.at_ms === m.at_ms));
  if (!fresh.length) return list;
  fresh.sort((a, b) => b.at_ms - a.at_ms || b.id - a.id);
  return [...fresh, ...list].slice(0, max);
}

export function acarsSummary(a: Acars | null, shown: number): string {
  if (!a) return '';
  if (!a.channels.length) return 'No ACARS channel in this capture; tune nearer 130.5 MHz';
  const on = `Listening on ${a.channels.length} channel${a.channels.length === 1 ? '' : 's'}`;
  return shown ? `${on} · ${shown} message${shown === 1 ? '' : 's'}` : `${on} · none yet`;
}

/** Who sent it, as far as a block says: the flight, else the registration. */
export function acarsWho(m: AcarsMessage): string {
  return m.flight || m.registration || 'Ground station';
}

/** "14:05:09" in the machine's zone. */
export function acarsTime(atMs: number): string {
  return new Date(atMs).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false });
}
