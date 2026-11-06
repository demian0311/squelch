// Booking a recording of a space station pass: a one-off `record` slot on the voice frequency that
// covers the pass with its slack, and follows the pass when fresh orbit data moves it.
import type { IssPass } from './api';
import { clock, slack } from './iss';
import type { Slot } from './schedule';

/** The slot's name in schedule.toml. */
export const PASS_NAME = 'Space station pass';
/** Voice and SSTV pictures; APRS on 145.825 is packet bursts, not worth an audio file. */
export const VOICE_MHZ = 145.8;
/** Seconds either side of the widened pass, so the minute rounding never cuts it. */
export const MARGIN_SECS = 60;
/** A booked slot belongs to the pass whose predicted rise is nearest, within this. Passes are ~90 min apart. */
export const MATCH_SECS = 45 * 60;
/** A moved pass moves its slot only past this, so a slowly growing slack does not rewrite the file every tick. */
export const DRIFT_SECS = 120;

export interface PassWindow {
  /** Unix seconds, on a whole minute. */
  start: number;
  end: number;
}

/** The minutes a recording must cover: the pass widened by the orbit data's slack and a margin, on whole minutes. */
export function passWindow(p: IssPass, age: number | null): PassWindow {
  const s = slack(age) + MARGIN_SECS;
  return { start: Math.floor((p.rise - s) / 60) * 60, end: Math.ceil((p.set + s) / 60) * 60 };
}

/** How many minutes early a booking starts (and late it ends) for orbit data this old. */
export const earlyMinutes = (age: number | null) => Math.ceil((slack(age) + MARGIN_SECS) / 60);

const two = (n: number) => String(n).padStart(2, '0');
const dateOf = (d: Date) => `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
const timeOf = (d: Date) => `${two(d.getHours())}:${two(d.getMinutes())}`;

/** The slot `s` would be if it covered `w` for pass `p`. */
function fitted(s: Slot, p: IssPass, w: PassWindow): Slot {
  const d = new Date(w.start * 1000);
  return { ...s, times: [timeOf(d)], days: [], date: dateOf(d), week: null, minutes: Math.max(1, Math.round((w.end - w.start) / 60)), pass: Math.round(p.rise) };
}

/** A new booking for `p`. */
export function passSlot(p: IssPass, age: number | null, id: string): Slot {
  const base: Slot = { id, name: PASS_NAME, mhz: VOICE_MHZ, mode: 'nfm', times: [], days: [], date: null, minutes: 1, action: 'record', go_back: true, pass: Math.round(p.rise) };
  return fitted(base, p, passWindow(p, age));
}

/** A slot's window in Unix seconds, from its one-off date and time; null when it is not a one-off. */
export function slotWindow(s: Slot): PassWindow | null {
  const m = s.date ? /^(\d{4})-(\d{2})-(\d{2})$/.exec(s.date) : null;
  const t = /^(\d{2}):(\d{2})$/.exec(s.times[0] ?? '');
  if (!m || !t || s.days.length) return null;
  const start = new Date(+m[1], +m[2] - 1, +m[3], +t[1], +t[2]).getTime() / 1000;
  return { start, end: start + s.minutes * 60 };
}

const isBooking = (s: Slot) => s.pass != null && s.action === 'record' && s.name === PASS_NAME;

/** The booking for pass `p`, if any. */
export function bookingOf(slots: Slot[], p: IssPass): Slot | undefined {
  let best: Slot | undefined;
  for (const s of slots) {
    if (!isBooking(s)) continue;
    const d = Math.abs((s.pass ?? 0) - p.rise);
    if (d <= MATCH_SECS && (!best || d < Math.abs((best.pass ?? 0) - p.rise))) best = s;
  }
  return best;
}

/** The list with pass `p` booked, or its booking removed when it has one. */
export function toggleBooking(slots: Slot[], p: IssPass, age: number | null, id: string): Slot[] {
  const have = bookingOf(slots, p);
  return have ? slots.filter((s) => s !== have) : [...slots, passSlot(p, age, id)];
}

/**
 * Bookings moved to follow their passes: a booking whose pass now rises somewhere else, or whose window
 * no longer covers the widened pass within `DRIFT_SECS`, is refitted. A booking already running (its
 * window has started by `now`) is left alone. Null when nothing changed.
 */
export function followPasses(slots: Slot[], passes: IssPass[], age: number | null, now: number): Slot[] | null {
  let changed = false;
  const next = slots.map((s) => {
    if (!isBooking(s)) return s;
    const w = slotWindow(s);
    if (w && w.start <= now) return s;
    const p = passes.find((x) => bookingOf([s], x) === s);
    if (!p) return s;
    const want = passWindow(p, age);
    const moved = Math.abs((s.pass ?? 0) - p.rise) >= 60;
    const off = !w || Math.abs(w.start - want.start) > DRIFT_SECS || Math.abs(w.end - want.end) > DRIFT_SECS;
    if (!moved && !off) return s;
    changed = true;
    return fitted(s, p, want);
  });
  return changed ? next : null;
}

/** "03:57 – 04:12 AM": the AM/PM once when both ends share it. */
export function recordSpan(w: PassWindow): string {
  const a = clock(w.start);
  const b = clock(w.end);
  const suffix = / [AP]M$/;
  const sa = suffix.exec(a)?.[0];
  return sa && a.endsWith(sa) && b.endsWith(sa) ? `${a.slice(0, -sa.length)} – ${b}` : `${a} – ${b}`;
}
