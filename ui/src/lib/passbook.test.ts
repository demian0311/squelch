import { describe, expect, it } from 'vitest';
import type { IssPass } from './api';
import { bookingOf, DRIFT_SECS, earlyMinutes, followPasses, MARGIN_SECS, passSlot, passWindow, slotWindow, recordSpan, toggleBooking, VOICE_MHZ } from './passbook';
import type { Slot } from './schedule';

// 2026-11-06 02:23:17 local, 10 min long, 40° high
const rise = new Date(2026, 10, 6, 2, 23, 17).getTime() / 1000;
const pass: IssPass = { rise, set: rise + 611, max_elevation: 40, rise_az: 300, set_az: 120 } as IssPass;
const later: IssPass = { ...pass, rise: rise + 5700, set: rise + 5700 + 540 };

describe('a booking covers the pass', () => {
  it('starts a minute early and ends a minute late on fresh data, on whole minutes', () => {
    const w = passWindow(pass, 0);
    expect(w.start).toBe(new Date(2026, 10, 6, 2, 22).getTime() / 1000);
    expect(w.end).toBe(new Date(2026, 10, 6, 2, 35).getTime() / 1000);
    expect(earlyMinutes(0)).toBe(1);
  });
  it('widens with old orbit data', () => {
    const w = passWindow(pass, 12); // slack 200 s
    expect(w.start).toBe(new Date(2026, 10, 6, 2, 18).getTime() / 1000);
    expect(earlyMinutes(12)).toBe(Math.ceil((200 + MARGIN_SECS) / 60));
  });
  it('is a one-off record slot on the voice frequency that goes back after, its marker a whole second', () => {
    const s = passSlot({ ...pass, rise: rise + 0.57 }, 0, 'x');
    expect(s).toMatchObject({ name: 'Space station pass', mhz: VOICE_MHZ, mode: 'nfm', action: 'record', go_back: true, date: '2026-11-06', times: ['02:22'], minutes: 13, days: [], pass: rise + 1 });
    expect(slotWindow(s)).toEqual(passWindow(pass, 0));
  });
});

describe('booking and unbooking', () => {
  it('finds the booking by the pass it was made for, not a neighbouring pass', () => {
    const slots = toggleBooking([], pass, 0, 'a');
    expect(bookingOf(slots, pass)?.id).toBe('a');
    expect(bookingOf(slots, later)).toBeUndefined();
  });
  it('a second toggle removes it and leaves other slots alone', () => {
    const net: Slot = { id: 'n', name: 'Net', mhz: 146.52, mode: 'nfm', times: ['19:30'], days: [2], minutes: 60, action: 'remind', go_back: false };
    const booked = toggleBooking([net], pass, 0, 'a');
    expect(booked).toHaveLength(2);
    expect(toggleBooking(booked, pass, 0, 'b')).toEqual([net]);
  });
  it('a hand-made record slot on 145.8 is not a booking', () => {
    const own: Slot = { id: 'o', name: 'ISS', mhz: VOICE_MHZ, mode: 'nfm', times: ['02:22'], days: [], date: '2026-11-06', minutes: 13, action: 'record', go_back: true };
    expect(bookingOf([own], pass)).toBeUndefined();
  });
});

describe('following the pass as orbit data refreshes', () => {
  const now = rise - 3600 * 5;
  it('leaves a booking alone while nothing moved', () => {
    const slots = toggleBooking([], pass, 0, 'a');
    expect(followPasses(slots, [pass, later], 0, now)).toBeNull();
    // a slack grown by less than the drift allowance changes nothing either
    expect(followPasses(slots, [pass, later], 4, now)).toBeNull();
  });
  it('moves the booking when the pass moves', () => {
    const slots = toggleBooking([], pass, 0, 'a');
    const shifted: IssPass = { ...pass, rise: rise + 4 * 60, set: pass.set + 4 * 60 };
    const next = followPasses(slots, [shifted, later], 0, now)!;
    expect(next).not.toBeNull();
    expect(next[0]).toMatchObject({ id: 'a', times: ['02:26'], pass: shifted.rise, minutes: 13 });
    expect(bookingOf(next, shifted)?.id).toBe('a');
  });
  it('widens the booking once the slack has grown past the drift allowance, and shrinks it on fresh data', () => {
    const slots = toggleBooking([], pass, 0, 'a');
    const wide = followPasses(slots, [pass], 20, now)!; // slack 360 s > DRIFT_SECS
    expect(slotWindow(wide[0])!.start).toBeLessThan(slotWindow(slots[0])!.start - DRIFT_SECS);
    expect(followPasses(wide, [pass], 0, now)![0].times).toEqual(['02:22']);
  });
  it('a booking already running is not moved', () => {
    const slots = toggleBooking([], pass, 0, 'a');
    const shifted: IssPass = { ...pass, rise: rise + 4 * 60, set: pass.set + 4 * 60 };
    expect(followPasses(slots, [shifted], 0, rise)).toBeNull();
  });
  it('a booking whose pass is no longer predicted stays as it is', () => {
    const slots = toggleBooking([], pass, 0, 'a');
    expect(followPasses(slots, [later], 0, now)).toBeNull();
  });
});

describe('the pill text', () => {
  it('says the AM or PM once when both ends share it', () => {
    expect(recordSpan(passWindow(pass, 0))).toMatch(/^02:22 – 02:35 AM$|^02:22 – 02:35$/);
    const noon: IssPass = { ...pass, rise: new Date(2026, 10, 6, 11, 55).getTime() / 1000, set: new Date(2026, 10, 6, 12, 4).getTime() / 1000 };
    expect(recordSpan(passWindow(noon, 0))).toMatch(/AM – .*PM$|^11:54 – 12:05$/);
  });
});
