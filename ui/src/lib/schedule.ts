// Times to tune in (nets, check-ins): when the next one starts, and which are close enough to remind about.
// Times are local wall-clock; `Date` does the daylight-saving arithmetic.
import type { Mode } from './api';

export interface Slot {
  id: string;
  name: string;
  mhz: number;
  mode: Mode;
  /** Local "HH:MM". */
  time: string;
  /** Days of the week it repeats on, 0 = Sunday. Empty = once, on `date`. */
  days: number[];
  /** Local "YYYY-MM-DD", for a slot that does not repeat. */
  date?: string;
}

/** Remind this long before a start. */
export const LEAD_MS = 5 * 60_000;
/** A start this recent still counts as "now", so a reminder is not lost to a slow tick. */
export const GRACE_MS = 2 * 60_000;

export const DAY_LETTERS = ['S', 'M', 'T', 'W', 'T', 'F', 'S'];
const DAY_NAMES = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

/** "HH:MM" to [hours, minutes], or null when it is not a time. */
export function parseTime(t: string): [number, number] | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(t);
  if (!m || +m[1] > 23 || +m[2] > 59) return null;
  return [+m[1], +m[2]];
}

/** The first start at or after `from`, or null (a one-off already past, or an unreadable time). */
export function nextStart(s: Slot, from: Date): Date | null {
  const time = parseTime(s.time);
  if (!time) return null;
  if (s.days.length === 0) {
    const m = s.date ? /^(\d{4})-(\d{2})-(\d{2})$/.exec(s.date) : null;
    if (!m) return null;
    const d = new Date(+m[1], +m[2] - 1, +m[3], time[0], time[1]);
    return d >= from ? d : null;
  }
  for (let i = 0; i <= 7; i++) {
    const d = new Date(from.getFullYear(), from.getMonth(), from.getDate() + i, time[0], time[1]);
    if (s.days.includes(d.getDay()) && d >= from) return d;
  }
  return null;
}

export interface Reminder {
  slot: Slot;
  start: Date;
  /** `soon` = within LEAD_MS ahead, `now` = it started within GRACE_MS. */
  phase: 'soon' | 'now';
  /** Same for the same start of the same slot, so a reminder can be shown once. */
  key: string;
}

/** Slots starting within LEAD_MS, or that started within GRACE_MS, soonest first. */
export function reminders(slots: Slot[], now: Date): Reminder[] {
  const out: Reminder[] = [];
  for (const slot of slots) {
    const start = nextStart(slot, new Date(now.getTime() - GRACE_MS));
    if (!start || start.getTime() - now.getTime() > LEAD_MS) continue;
    out.push({ slot, start, phase: start <= now ? 'now' : 'soon', key: `${slot.id}@${start.getTime()}` });
  }
  return out.sort((a, b) => a.start.getTime() - b.start.getTime());
}

/** "in 4 min", "today 19:00", "Tue 19:00", "Nov 12 19:00". */
export function when(start: Date, now: Date): string {
  const hhmm = `${String(start.getHours()).padStart(2, '0')}:${String(start.getMinutes()).padStart(2, '0')}`;
  const mins = Math.round((start.getTime() - now.getTime()) / 60_000);
  if (mins <= 0) return 'now';
  if (mins < 60) return `in ${mins} min`;
  const midnight = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((midnight(start) - midnight(now)) / 86_400_000);
  if (days === 0) return `today ${hhmm}`;
  if (days === 1) return `tomorrow ${hhmm}`;
  if (days < 7) return `${DAY_NAMES[start.getDay()]} ${hhmm}`;
  return `${start.toLocaleString('en-US', { month: 'short', day: 'numeric' })} ${hhmm}`;
}

/** Slots in the order they next start; ones that never will again go last. */
export function byNext(slots: Slot[], now: Date): Slot[] {
  const t = (s: Slot) => nextStart(s, new Date(now.getTime() - GRACE_MS))?.getTime() ?? Infinity;
  return [...slots].sort((a, b) => t(a) - t(b));
}

/** A new slot on the given frequency, once, at the next quarter hour after `now`. */
export function newSlot(mhz: number, mode: Mode, now: Date): Slot {
  const d = new Date(Math.ceil((now.getTime() + 60_000) / 900_000) * 900_000);
  const p = (n: number) => String(n).padStart(2, '0');
  const id = `${now.getTime().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  return { id, name: '', mhz, mode, time: `${p(d.getHours())}:${p(d.getMinutes())}`, days: [], date: `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}` };
}

/** The slots on a frequency: a time belongs to a bookmark by being on it. */
export function slotsAt(slots: Slot[], mhz: number): Slot[] {
  return slots.filter((s) => Math.abs(s.mhz - mhz) < 0.0005);
}

/** The slots on no bookmark's frequency. */
export function loneSlots(slots: Slot[], marks: { mhz: number }[]): Slot[] {
  return slots.filter((s) => !marks.some((m) => slotsAt([s], m.mhz).length));
}

/** The slot at a new time of day. A one-off whose date would now be past moves to the next day that time comes. */
export function withTime(s: Slot, time: string, now: Date): Slot {
  const next = { ...s, time };
  if (s.days.length || nextStart(next, now)) return next;
  const t = parseTime(time);
  if (!t) return next;
  const d = new Date(now.getFullYear(), now.getMonth(), now.getDate(), t[0], t[1]);
  if (d < now) d.setDate(d.getDate() + 1);
  const p = (n: number) => String(n).padStart(2, '0');
  return { ...next, date: `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}` };
}

/** Read the stored list, dropping anything malformed. */
export function readSlots(raw: string | null): Slot[] {
  try {
    const v = JSON.parse(raw ?? '[]');
    return Array.isArray(v) ? v.filter((s) => s && typeof s.id === 'string' && typeof s.mhz === 'number' && typeof s.time === 'string' && Array.isArray(s.days)) : [];
  } catch {
    return [];
  }
}
