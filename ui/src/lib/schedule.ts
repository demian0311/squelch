// Times to tune in (nets, check-ins): when the next one starts, and which are close enough to remind about.
// Times are local wall-clock; `Date` does the daylight-saving arithmetic.
import type { Mode } from './api';

export interface Slot {
  /** Stays the same while the page is open, so a form keeps its focus across saves. */
  id: string;
  /** Its place in schedule.toml when last read; none until first saved. */
  at?: number;
  name: string;
  mhz: number;
  mode: Mode;
  /** Local "HH:MM". */
  times: string[];
  /** Days of the week it repeats on, 0 = Sunday. Empty = once, on `date`. */
  days: number[];
  /** Local "YYYY-MM-DD", for a slot that does not repeat. */
  date?: string | null;
  /** Only this week of the month, 1 to 4, or 5 for the last. */
  week?: number | null;
  minutes: number;
  action: Action;
  /** When it ends, back to the frequency it took over from. */
  go_back: boolean;
}

export type Action = 'remind' | 'tune' | 'record';

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

/** Whether `d` is in the week of the month `week` asks for: 1 to 4, or 5 for the last. */
function inWeek(d: Date, week: number | null | undefined): boolean {
  if (!week) return true;
  if (week === 5) return new Date(d.getFullYear(), d.getMonth(), d.getDate() + 7).getMonth() !== d.getMonth();
  return Math.ceil(d.getDate() / 7) === week;
}

/** The first start at or after `from`, or null (a one-off already past, or no readable time). */
export function nextStart(s: Slot, from: Date): Date | null {
  const times = s.times.map(parseTime).filter((t) => t !== null).sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  if (s.days.length === 0) {
    const m = s.date ? /^(\d{4})-(\d{2})-(\d{2})$/.exec(s.date) : null;
    if (!m) return null;
    for (const t of times) {
      const d = new Date(+m[1], +m[2] - 1, +m[3], t[0], t[1]);
      if (d >= from) return d;
    }
    return null;
  }
  // far enough for a monthly one to come round
  for (let i = 0; i <= 62; i++) {
    for (const t of times) {
      const d = new Date(from.getFullYear(), from.getMonth(), from.getDate() + i, t[0], t[1]);
      if (s.days.includes(d.getDay()) && inWeek(d, s.week) && d >= from) return d;
    }
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
    out.push({ slot, start, phase: start <= now ? 'now' : 'soon', key: `${slot.mhz}@${start.getTime()}` });
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

/** The computer wakes this long before a planned start, so tuning settles and the hold is in place. */
export const WAKE_LEAD_MS = 90_000;

export interface Window {
  slot: Slot;
  start: Date;
  end: Date;
  /** Same as the reminder's key for this start. */
  key: string;
}

/** The first run of a slot that has not ended by `now`. */
function runOf(s: Slot, now: Date): Window | null {
  const start = nextStart(s, new Date(now.getTime() - s.minutes * 60_000 + 1));
  return start && { slot: s, start, end: new Date(start.getTime() + s.minutes * 60_000), key: `${s.id}@${start.getTime()}` };
}

/** Slots that act by themselves (tune, record) running at `now`, earliest start first. */
export function openWindows(slots: Slot[], now: Date): Window[] {
  return slots
    .filter((s) => s.action !== 'remind')
    .map((s) => runOf(s, now))
    .filter((w): w is Window => !!w && w.start <= now)
    .sort((a, b) => a.start.getTime() - b.start.getTime());
}

/** When to wake the computer next: ahead of the next start that acts by itself and is not already within the lead. */
export function nextWake(slots: Slot[], now: Date): Date | null {
  const after = new Date(now.getTime() + WAKE_LEAD_MS);
  const starts = slots.filter((s) => s.action !== 'remind').map((s) => nextStart(s, after)?.getTime() ?? Infinity);
  const first = Math.min(Infinity, ...starts);
  return first === Infinity ? null : new Date(first - WAKE_LEAD_MS);
}

/** Until when the computer must stay awake: through any run now open or starting within the lead. */
export function awakeUntil(slots: Slot[], now: Date): Date | null {
  const ends = slots
    .filter((s) => s.action !== 'remind')
    .map((s) => runOf(s, now))
    .filter((w): w is Window => !!w && w.start.getTime() - WAKE_LEAD_MS <= now.getTime())
    .map((w) => w.end.getTime());
  return ends.length ? new Date(Math.max(...ends)) : null;
}

/** Slots in the order they next start; ones that never will again go last. */
export function byNext(slots: Slot[], now: Date): Slot[] {
  const t = (s: Slot) => nextStart(s, new Date(now.getTime() - GRACE_MS))?.getTime() ?? Infinity;
  return [...slots].sort((a, b) => t(a) - t(b));
}

const newId = () => `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;

/** A new slot on the given frequency, with no time yet. */
export function newSlot(name: string, mhz: number, mode: Mode, now: Date): Slot {
  return { id: newId(), name, mhz, mode, times: [], days: [], date: dateOf(now), minutes: 60, action: 'remind', go_back: false };
}

const dateOf = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

/** Every 3 hours from midnight. */
export const EVERY_3_HOURS = [0, 3, 6, 9, 12, 15, 18, 21].map((h) => `${String(h).padStart(2, '0')}:00`);
export const EVERY_DAY = [0, 1, 2, 3, 4, 5, 6];

/** The 3-3-3 plan (every 3 hours, for 3 minutes) on whichever frequency the slot is on. */
export function plan333(s: Slot): Slot {
  return { ...s, name: '3-3-3', times: EVERY_3_HOURS, days: EVERY_DAY, date: null, week: null, minutes: 3, action: 'tune' };
}

/** The slots on a frequency: a time belongs to a bookmark by being on it. */
export function slotsAt(slots: Slot[], mhz: number): Slot[] {
  return slots.filter((s) => Math.abs(s.mhz - mhz) < 0.0005);
}

/** The slots on no bookmark's frequency. */
export function loneSlots(slots: Slot[], marks: { mhz: number }[]): Slot[] {
  return slots.filter((s) => !marks.some((m) => slotsAt([s], m.mhz).length));
}

/** The slot at new times of day. A one-off whose date would now be past moves to the next day one comes. */
export function withTimes(s: Slot, times: string[], now: Date): Slot {
  const next = { ...s, times: [...new Set(times)].sort() };
  if (s.days.length || !next.times.length || nextStart(next, now)) return next;
  const day = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const today = { ...next, date: dateOf(day) };
  if (nextStart(today, now)) return today;
  day.setDate(day.getDate() + 1);
  return { ...next, date: dateOf(day) };
}

/** A slot as the file gives it. */
export type ReadSlot = Omit<Slot, 'id'>;

/** The list as read, each slot keeping the id of the same slot in `prev`: one that reads the same, else one at its place in the file. */
export function withIds(prev: Slot[], read: ReadSlot[]): Slot[] {
  const free = [...prev];
  const claim = (match: (p: Slot) => boolean) => {
    const i = free.findIndex(match);
    return i < 0 ? undefined : free.splice(i, 1)[0].id;
  };
  const alike = (a: ReadSlot, b: ReadSlot) => a.name === b.name && a.mhz === b.mhz && a.mode === b.mode && a.times.join() === b.times.join() && (a.date ?? '') === (b.date ?? '') && a.days.join() === b.days.join() && (a.week ?? 0) === (b.week ?? 0) && a.minutes === b.minutes && a.action === b.action && a.go_back === b.go_back;
  const same = read.map((s) => claim((p) => alike(p, s)));
  return read.map((s, i) => ({ ...s, id: same[i] ?? claim((p) => p.at === s.at) ?? newId() }));
}
