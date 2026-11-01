import { describe, expect, it } from 'vitest';
import { KEEP_MIN, activity, ago, listened, liveWalkies, parseHistory, record, sideView, sidesIn, walkieAt, type Minute } from './walkie';

const MIN = 60_000;
const T0 = 29_000_000 * MIN;

describe('walkie channels', () => {
  it('finds the channel or repeater input a signal is on', () => {
    expect(walkieAt(462_625_300)?.key).toBe('18');
    expect(walkieAt(467_625_000)?.key).toBe('18R');
    expect(walkieAt(467_587_500)?.key).toBe('9');
    expect(walkieAt(462_745_000)).toBeUndefined();
  });
  it('keeps the strongest signal per channel', () => {
    expect([...liveWalkies([{ hz: 462_675_000, snr_db: 20 }, { hz: 462_675_200, snr_db: 30 }, { hz: 100e6, snr_db: 40 }])]).toEqual([['20', 30]]);
  });
  it('zooms to the side holding a channel', () => {
    expect(sideView(462_600_000)).toEqual([462_525_000, 462_750_000]);
    expect(sideView(467_625_000)).toEqual([467_525_000, 467_750_000]);
  });
  it('hears a side only when all of it is in the capture', () => {
    expect(sidesIn(461.5e6, 463.5e6)).toEqual([462]);
    expect(sidesIn(462.6e6, 464e6)).toEqual([]);
    expect(sidesIn(466.8e6, 468.4e6)).toEqual([467]);
  });
});

describe('walkie history', () => {
  it('records each minute once and says when a new one starts', () => {
    const h: Minute[] = [];
    expect(record(h, T0, [462], ['3'])).toBe(true);
    expect(record(h, T0 + 30_000, [462], ['3', '18'])).toBe(false);
    expect(h).toEqual([{ t: T0 / MIN, sides: 1, on: ['3', '18'] }]);
    expect(record(h, T0 + MIN, [], ['3'])).toBe(true);
    expect(h[1]).toEqual({ t: T0 / MIN + 1, sides: 0, on: [] });
  });
  it('never marks a channel on the side not listened to', () => {
    const h: Minute[] = [];
    record(h, T0, [462], ['9', '18R', '18']);
    expect(h[0].on).toEqual(['18']);
  });
  it('drops minutes older than six hours', () => {
    const h: Minute[] = [];
    for (let i = 0; i < KEEP_MIN + 10; i++) record(h, T0 + i * MIN, [462], []);
    expect(h.length).toBe(KEEP_MIN);
    expect(h[0].t).toBe(T0 / MIN + 10);
  });
  it('reads back saved minutes, skipping old and broken ones', () => {
    const now = T0 + 10 * MIN;
    const list = [{ t: T0 / MIN - KEEP_MIN, sides: 1, on: [] }, { t: T0 / MIN, sides: 1, on: ['3'] }, { t: 'x' }];
    expect(parseHistory(list, now)).toEqual([{ t: T0 / MIN, sides: 1, on: ['3'] }]);
    expect(parseHistory('nope', now)).toEqual([]);
  });
  it('counts only minutes something was listened to', () => {
    const h: Minute[] = [];
    record(h, T0, [462], []);
    record(h, T0 + MIN, [], []);
    record(h, T0 + 2 * MIN, [467], []);
    expect(listened(h)).toBe(2);
  });
});

describe('activity grid', () => {
  it('shows a minute per column for a short history, and gaps where a side was not heard', () => {
    const h: Minute[] = [];
    for (let i = 0; i < 10; i++) record(h, T0 + i * MIN, i < 5 ? [462] : [467], i % 2 ? ['3', '9'] : []);
    const a = activity(h, T0 + 9 * MIN);
    expect(a.spanMin).toBe(10);
    const row = (k: string) => a.rows.find((r) => r.channel.key === k)!;
    expect(row('3').cells).toEqual([0, 1, 0, 1, 0, null, null, null, null, null]);
    expect(row('9').cells).toEqual([null, null, null, null, null, 1, 0, 1, 0, 1]);
    expect(row('3').last).toBe(T0 / MIN + 3);
  });
  it('folds six hours into at most sixty columns', () => {
    const h: Minute[] = [];
    for (let i = 0; i < KEEP_MIN; i++) record(h, T0 + i * MIN, [462], i < 6 ? ['18'] : []);
    const a = activity(h, T0 + (KEEP_MIN - 1) * MIN);
    expect(a.spanMin).toBe(KEEP_MIN);
    const cells = a.rows.find((r) => r.channel.key === '18')!.cells;
    expect(cells.length).toBe(60);
    expect(cells[0]).toBe(1);
    expect(cells[1]).toBe(0);
  });
});

describe('ago', () => {
  it('says how long since', () => {
    const now = T0 + 130 * MIN;
    expect(ago(T0 / MIN + 130, now)).toBe('now');
    expect(ago(T0 / MIN + 126, now)).toBe('4 min');
    expect(ago(T0 / MIN, now)).toBe('2 h 10 min');
    expect(ago(null, now)).toBe('');
  });
});
