// The Recordings screen's pure parts: reading a file name, grouping by day, labels.
import type { Mode, SavedRecording } from './api';

/** `…/2026-11-01T153012Z 101.100MHz wfm.opus` → when (Unix seconds), Hz and mode; null for any other file. */
export function parseRecordingName(file: string): { at: number; hz: number; mode: Mode } | null {
  const m = /(\d{4})-(\d\d)-(\d\d)T(\d\d)(\d\d)(\d\d)Z (\d+\.\d+)MHz (wfm|nfm|am|usb|lsb|none|p25)\.opus$/.exec(file);
  if (!m) return null;
  const at = Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]) / 1000;
  return { at, hz: Math.round(+m[7] * 1e6), mode: m[8] as Mode };
}

/** `m:ss`, hours shown past an hour. */
export function clockLength(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

/** The local time of day, `13:35`. */
export function timeOfDay(at: number): string {
  const d = new Date(at * 1000);
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

/** Today, Yesterday, else the weekday and date; `now` in Unix seconds. */
export function dayLabel(at: number, now: number): string {
  const d = new Date(at * 1000);
  const today = new Date(now * 1000);
  const dayOf = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((dayOf(today) - dayOf(d)) / 86_400_000);
  if (days === 0) return 'Today';
  if (days === 1) return 'Yesterday';
  const text = d.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long', year: d.getFullYear() === today.getFullYear() ? undefined : 'numeric' });
  return text;
}

export interface Day {
  label: string;
  rows: SavedRecording[];
}

/** Recordings by local day, newest day first, newest first within a day. */
export function byDay(list: SavedRecording[], now: number): Day[] {
  const sorted = [...list].sort((a, b) => b.at - a.at);
  const out: Day[] = [];
  for (const r of sorted) {
    const label = dayLabel(r.at, now);
    const last = out[out.length - 1];
    if (last && last.label === label) last.rows.push(r);
    else out.push({ label, rows: [r] });
  }
  return out;
}

/** The one after `file` in the list's order (older), for playing on. */
export function nextAfter(list: SavedRecording[], file: string): SavedRecording | null {
  const sorted = [...list].sort((a, b) => b.at - a.at);
  const i = sorted.findIndex((r) => r.file === file);
  return i >= 0 && i + 1 < sorted.length ? sorted[i + 1] : null;
}

/** Bytes as the list prints them. */
export function sizeLabel(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1e3))} kB`;
}
