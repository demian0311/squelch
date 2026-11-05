// DMR mode: what the top bar and the panel say about the two slots and their calls.
import type { Dmr, DmrCall } from './api';

/** Who is talking to whom: "Talkgroup 31088 ← radio 3104321", or what is known so far. */
export function callSay(c: DmrCall): string {
  const to = c.dst === null ? 'Reading who is calling…' : c.group ? `Talkgroup ${c.dst}` : `Radio ${c.dst}, private`;
  const from = c.src === null ? '' : ` ← radio ${c.src}`;
  return c.dst === null ? to : `${to}${from}`;
}

/** The state word after a call: playing, waiting its turn, or muted and why. */
export function callState(d: Dmr, slot: 0 | 1): string {
  const s = d.slots[slot];
  if (!s.call) return 'quiet';
  if (s.call.encrypted) return 'encrypted · muted';
  if (d.voice_error) return 'cannot play';
  return s.playing ? 'playing' : 'waiting · the other slot plays';
}

/** One line for the top bar and the folded panel. */
export function dmrSummary(d: Dmr): string {
  const live = d.slots.find((s) => s.call && s.playing)?.call;
  if (live) return `${callSay(live)} · slot ${live.slot}`;
  const held = d.slots.find((s) => s.call)?.call;
  if (held) return `${callSay(held)} · ${held.encrypted ? 'encrypted, muted' : 'slot ' + held.slot}`;
  if (!d.heard) return d.colour_code === null ? 'No DMR heard here yet' : 'DMR quiet · waiting for a call';
  const cc = d.colour_code === null ? '' : ` · colour code ${d.colour_code}`;
  return `DMR heard${cc} · waiting for a call`;
}

/** "12 s", "1:05" for how long a call ran. */
export function lengthSay(c: DmrCall): string {
  const s = Math.max(0, Math.round(c.last_secs - c.start_secs));
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** "just now", "40 s ago", "3 min ago". */
export function agoSay(nowSecs: number, c: DmrCall): string {
  const s = Math.max(0, nowSecs - c.last_secs);
  if (s < 5) return 'just now';
  if (s < 60) return `${Math.round(s)} s ago`;
  return `${Math.round(s / 60)} min ago`;
}
