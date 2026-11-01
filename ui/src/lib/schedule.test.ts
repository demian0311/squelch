import { describe, expect, it } from 'vitest';
import { GRACE_MS, LEAD_MS, byNext, loneSlots, newSlot, nextStart, parseTime, reminders, slotsAt, when, withTime, withIds, type Slot } from './schedule';

const slot = (o: Partial<Slot>): Slot => ({ id: 'a', name: 'Net', mhz: 146.52, mode: 'nfm', time: '19:00', days: [], ...o });
// local times, so the tests hold in any zone
const at = (y: number, mo: number, d: number, h = 0, mi = 0) => new Date(y, mo - 1, d, h, mi);
const EVERY_DAY = [0, 1, 2, 3, 4, 5, 6];

describe('parseTime', () => {
  it('reads HH:MM and rejects the rest', () => {
    expect(parseTime('7:05')).toEqual([7, 5]);
    expect(parseTime('24:00')).toBeNull();
    expect(parseTime('19:60')).toBeNull();
    expect(parseTime('soon')).toBeNull();
  });
});

describe('nextStart', () => {
  it('a one-off is its date, and gone once past', () => {
    const s = slot({ date: '2026-11-03' });
    expect(nextStart(s, at(2026, 11, 1))).toEqual(at(2026, 11, 3, 19));
    expect(nextStart(s, at(2026, 11, 3, 19, 1))).toBeNull();
    expect(nextStart(slot({}), at(2026, 11, 1))).toBeNull();
  });

  it('a weekly slot waits for its day, today if still ahead', () => {
    const tue = slot({ days: [2] });
    expect(nextStart(tue, at(2026, 11, 1))).toEqual(at(2026, 11, 3, 19)); // Sun 1 Nov 2026
    expect(nextStart(tue, at(2026, 11, 3, 18))).toEqual(at(2026, 11, 3, 19));
    expect(nextStart(tue, at(2026, 11, 3, 20))).toEqual(at(2026, 11, 10, 19));
  });

  it('a daily slot rolls to tomorrow after its time', () => {
    expect(nextStart(slot({ days: EVERY_DAY }), at(2026, 11, 1, 19, 30))).toEqual(at(2026, 11, 2, 19));
  });
});

describe('reminders', () => {
  const s = slot({ days: EVERY_DAY });
  it('fires inside the lead, not before', () => {
    expect(reminders([s], at(2026, 11, 1, 18, 54))).toEqual([]);
    const r = reminders([s], new Date(at(2026, 11, 1, 19).getTime() - LEAD_MS));
    expect(r.map((x) => x.phase)).toEqual(['soon']);
  });

  it('says now at the start and for a short grace after, then stops', () => {
    expect(reminders([s], at(2026, 11, 1, 19))[0].phase).toBe('now');
    expect(reminders([s], new Date(at(2026, 11, 1, 19).getTime() + GRACE_MS - 1))[0].phase).toBe('now');
    expect(reminders([s], new Date(at(2026, 11, 1, 19).getTime() + GRACE_MS + 1))).toEqual([]);
  });

  it('keeps one key per start, a new one the next day', () => {
    const a = reminders([s], at(2026, 11, 1, 18, 57))[0].key;
    expect(reminders([s], at(2026, 11, 1, 19, 1))[0].key).toBe(a);
    expect(reminders([s], at(2026, 11, 2, 18, 57))[0].key).not.toBe(a);
  });
});

describe('when', () => {
  const now = at(2026, 11, 1, 12);
  it('describes how far off', () => {
    expect(when(at(2026, 11, 1, 12, 4), now)).toBe('in 4 min');
    expect(when(at(2026, 11, 1, 19), now)).toBe('today 19:00');
    expect(when(at(2026, 11, 2, 7, 5), now)).toBe('tomorrow 07:05');
    expect(when(at(2026, 11, 4, 19), now)).toBe('Wed 19:00');
  });
});

describe('byNext', () => {
  it('orders by next start with finished one-offs last', () => {
    const now = at(2026, 11, 1, 12);
    const done = slot({ id: 'done', date: '2026-10-01' });
    const later = slot({ id: 'later', date: '2026-11-05' });
    const sooner = slot({ id: 'sooner', date: '2026-11-02' });
    expect(byNext([done, later, sooner], now).map((s) => s.id)).toEqual(['sooner', 'later', 'done']);
  });
});

describe('slots on a bookmark', () => {
  const a = slot({ id: 'a', mhz: 146.52 });
  const b = slot({ id: 'b', mhz: 146.94 });
  it('finds them by frequency', () => {
    expect(slotsAt([a, b], 146.5201).map((s) => s.id)).toEqual(['a']);
    expect(slotsAt([a, b], 146.53)).toEqual([]);
  });
  it('leaves the rest as lone', () => {
    expect(loneSlots([a, b], [{ mhz: 146.52 }]).map((s) => s.id)).toEqual(['b']);
  });
});

describe('withTime', () => {
  const now = at(2026, 11, 1, 12, 0);
  it('keeps a one-off date that is still ahead', () => {
    expect(withTime(slot({ days: [], date: '2026-11-01' }), '13:00', now).date).toBe('2026-11-01');
  });
  it('moves a one-off already past to tomorrow', () => {
    const s = withTime(slot({ days: [], date: '2026-11-01' }), '11:00', now);
    expect(s.time).toBe('11:00');
    expect(s.date).toBe('2026-11-02');
  });
  it('leaves a weekly slot alone', () => {
    expect(withTime(slot({ days: [2], date: undefined }), '11:00', now).date).toBeUndefined();
  });
});

describe('newSlot', () => {
  it('starts at the next quarter hour, once', () => {
    const s = newSlot(146.52, 'nfm', at(2026, 11, 1, 12, 7));
    expect(s.time).toBe('12:15');
    expect(s.date).toBe('2026-11-01');
    expect(nextStart(s, at(2026, 11, 1, 12, 7))).toEqual(at(2026, 11, 1, 12, 15));
  });
});

describe('withIds', () => {
  const read = (s: Slot) => {
    const { id: _, ...r } = s;
    return r;
  };
  it('keeps the id of a slot saved for the first time', () => {
    const fresh = slot({ id: 'draft', at: undefined });
    expect(withIds([fresh], [{ ...read(fresh), at: 0 }])).toEqual([{ ...fresh, at: 0 }]);
  });
  it('follows a slot edited in the file, by its place', () => {
    const a = slot({ id: 'a', at: 0, name: 'One' });
    const b = slot({ id: 'b', at: 1, name: 'Two' });
    expect(withIds([a, b], [read(a), { ...read(b), name: 'Renamed' }]).map((s) => s.id)).toEqual(['a', 'b']);
  });
  it('follows a slot that moved up when one before it went', () => {
    const a = slot({ id: 'a', at: 0, name: 'One' });
    const b = slot({ id: 'b', at: 1, name: 'Two' });
    const out = withIds([a, b], [{ ...read(b), at: 0 }]);
    expect(out.map((s) => [s.id, s.at])).toEqual([['b', 0]]);
  });
  it('gives a slot added in the file a new id', () => {
    const a = slot({ id: 'a', at: 0, name: 'One' });
    const out = withIds([a], [read(a), { ...read(a), name: 'Two', at: 1 }]);
    expect(out[0].id).toBe('a');
    expect(out[1].id).not.toBe('a');
  });
});
