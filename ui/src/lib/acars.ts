// ACARS: the panel's list of aircraft messages.
import type { Acars, AcarsMessage } from './api';

/** Messages the panel keeps; each status carries only the last few seconds' worth. */
export const MAX_MESSAGES = 500;
/** How far back the panel starts from the log, days. */
export const LOG_DAYS = 14;
/** Where the capture sits on the band: every US channel but the far 129.125 MHz fits. */
export const ACARS_CENTRE_HZ = 130_500_000;

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

/** What a label means, where a published source says (airframesio's acars-message-documentation, fetched 2026-11-02). */
const LABELS: Record<string, string> = {
  _d: 'Acknowledgement',
  Q0: 'Link test',
  SQ: 'Ground station beacon',
  SA: 'Media advisory',
  MA: 'Media advisory',
  RA: 'Command',
  RB: 'Response',
  HX: 'Undelivered uplink',
  H1: 'Message to or from a terminal',
  H2: 'Weather report',
  QF: 'Off report with destination',
  QQ: 'Off report',
  '4T': 'Aircraft status',
  '4J': 'Position report',
  '5U': 'Weather request',
  '16': 'Position',
  '20': 'Position report',
  '21': 'Position report',
  '22': 'Position report',
  '3F': 'ETA',
  '40': 'Free text from the ground',
  '44': 'Off, on, in, out and positions',
  '83': 'Airline defined',
  BA: 'Air traffic control',
  A1: 'Oceanic clearance',
  A3: 'Departure clearance',
  B1: 'Oceanic clearance request',
  B2: 'Oceanic clearance acknowledgement',
  B3: 'Departure clearance request',
  B4: 'Departure clearance readback',
  C1: 'Fuel request',
};

export const labelName = (label: string) => LABELS[label] ?? '';

const signed = (v: number, hemi: string, neg: string) => (hemi === neg ? -v : v);
const valid = (lat: number, lon: number) => (Math.abs(lat) <= 90 && Math.abs(lon) <= 180 ? { lat, lon } : null);

/**
 * A position in the text, from the layouts documented per label: POS with degrees and tenths of minutes
 * (20, H1), POS in decimal degrees (21), degrees minutes seconds (22), decimal degrees anywhere (16).
 * Null for everything else; a label's other layouts are airline-specific.
 */
export function acarsPosition(m: { label: string; text: string }): { lat: number; lon: number } | null {
  const t = m.text;
  let r: RegExpMatchArray | null;
  if ((m.label === '20' || m.label === 'H1') && (r = t.match(/^(?:#\w+\/)?POS([NS])(\d{2})(\d{3})([EW])(\d{3})(\d{3})/))) {
    return valid(signed(+r[2] + +r[3] / 600, r[1], 'S'), signed(+r[5] + +r[6] / 600, r[4], 'W'));
  }
  if (m.label === '21' && (r = t.match(/^POS\s*([NS])\s*(\d+\.\d+)\s*([EW])\s*(\d+\.\d+)/))) {
    return valid(signed(+r[2], r[1], 'S'), signed(+r[4], r[3], 'W'));
  }
  if (m.label === '22' && (r = t.match(/^([NS])\s*(\d{2})(\d{2})(\d{2})([EW])\s*(\d{2,3})(\d{2})(\d{2})\b/))) {
    return valid(signed(+r[2] + +r[3] / 60 + +r[4] / 3600, r[1], 'S'), signed(+r[6] + +r[7] / 60 + +r[8] / 3600, r[5], 'W'));
  }
  if (m.label === '16' && (r = t.match(/([NS])\s*(\d{1,2}\.\d{3})\s*[/ ]\s*([EW])\s*(\d{1,3}\.\d{3})/))) {
    return valid(signed(+r[2], r[1], 'S'), signed(+r[4], r[3], 'W'));
  }
  return null;
}

/** Blocks of one message arrive this close together, and a repeat of a block this soon is a retransmission. */
const SAME_MESSAGE_MS = 120_000;

/**
 * Blocks (newest first) as messages (newest first): a block ending in ETB is continued by the next from the
 * same aircraft with the same label and message number stem, and an unanswered block sent again counts once.
 * Published sources do not spell this rule out; it is how the fields read on air.
 */
export function joinBlocks(list: AcarsMessage[]): AcarsMessage[] {
  const out: AcarsMessage[] = [];
  const open = new Map<string, AcarsMessage>();
  for (let i = list.length - 1; i >= 0; i--) {
    const m = list[i];
    const key = `${m.registration}|${m.label}|${m.msn.slice(0, 3)}`;
    const o = open.get(key);
    if (o && m.at_ms - o.at_ms <= SAME_MESSAGE_MS && m.text && o.text.endsWith(m.text)) continue;
    if (o && o.more && m.at_ms - o.at_ms <= SAME_MESSAGE_MS) {
      const joined = { ...o, text: o.text + m.text, more: m.more, at_ms: m.at_ms };
      out[out.indexOf(o)] = joined;
      open.set(key, joined);
      continue;
    }
    const copy = { ...m };
    out.push(copy);
    open.set(key, copy);
  }
  return out.reverse();
}

/** Messages holding every word of `query` in who sent them, their label or their text. */
export function filterMessages(list: AcarsMessage[], query: string): AcarsMessage[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return list;
  return list.filter((m) => {
    const hay = [m.flight, m.registration, m.label, labelName(m.label), m.text].join(' ').toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
