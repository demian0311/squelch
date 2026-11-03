import { describe, expect, it } from 'vitest';
import { BOARDS, KEEP_MIN, activity, ago, boardFor, channelOn, listened, liveOn, parseHistory, record, sideOf, sideView, sidesIn, stripOf, type Minute } from './board';

const W = boardFor('walkie')!;
const MIN = 60_000;
const T0 = 29_000_000 * MIN;

describe('walkie channels', () => {
  it('finds the channel or repeater input a signal is on', () => {
    expect(channelOn(W, 462_625_300)?.key).toBe('18');
    expect(channelOn(W, 467_625_000)?.key).toBe('18R');
    expect(channelOn(W, 467_587_500)?.key).toBe('9');
    expect(channelOn(W, 462_745_000)).toBeUndefined();
  });
  it('keeps the strongest signal per channel', () => {
    expect([...liveOn(W, [{ hz: 462_675_000, snr_db: 20 }, { hz: 462_675_200, snr_db: 30 }, { hz: 100e6, snr_db: 40 }])]).toEqual([['20', 30]]);
  });
  it('zooms to the side holding a channel', () => {
    expect(sideView(W, 462_600_000)).toEqual([462_525_000, 462_750_000]);
    expect(sideView(W, 467_625_000)).toEqual([467_525_000, 467_750_000]);
  });
  it('hears a side only when all of it is in the capture', () => {
    expect(sidesIn(W, 461.5e6, 463.5e6)).toEqual([0]);
    expect(sidesIn(W, 462.6e6, 464e6)).toEqual([]);
    expect(sidesIn(W, 466.8e6, 468.4e6)).toEqual([1]);
  });
});

describe('walkie history', () => {
  it('records each minute once and says when a new one starts', () => {
    const h: Minute[] = [];
    expect(record(W, h, T0, [0], ['3'])).toBe(true);
    expect(record(W, h, T0 + 30_000, [0], ['3', '18'])).toBe(false);
    expect(h).toEqual([{ t: T0 / MIN, sides: 1, on: ['3', '18'] }]);
    expect(record(W, h, T0 + MIN, [], ['3'])).toBe(true);
    expect(h[1]).toEqual({ t: T0 / MIN + 1, sides: 0, on: [] });
  });
  it('never marks a channel on the side not listened to', () => {
    const h: Minute[] = [];
    record(W, h, T0, [0], ['9', '18R', '18']);
    expect(h[0].on).toEqual(['18']);
  });
  it('drops minutes older than six hours', () => {
    const h: Minute[] = [];
    for (let i = 0; i < KEEP_MIN + 10; i++) record(W, h, T0 + i * MIN, [0], []);
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
    record(W, h, T0, [0], []);
    record(W, h, T0 + MIN, [], []);
    record(W, h, T0 + 2 * MIN, [1], []);
    expect(listened(h, T0 + 2 * MIN)).toBe(2);
    expect(listened(h, T0 + 2 * MIN + KEEP_MIN * MIN)).toBe(0);
  });
});

describe('activity grid', () => {
  it('shows a minute per column for a short history, and gaps where a side was not heard', () => {
    const h: Minute[] = [];
    for (let i = 0; i < 10; i++) record(W, h, T0 + i * MIN, i < 5 ? [0] : [1], i % 2 ? ['3', '9'] : []);
    const a = activity(W, h, T0 + 9 * MIN);
    expect(a.spanMin).toBe(10);
    const row = (k: string) => a.rows.find((r) => r.channel.key === k)!;
    expect(row('3').cells).toEqual([0, 1, 0, 1, 0, null, null, null, null, null]);
    expect(row('9').cells).toEqual([null, null, null, null, null, 1, 0, 1, 0, 1]);
    expect(row('3').last).toBe(T0 / MIN + 3);
  });
  it('folds six hours into at most sixty columns', () => {
    const h: Minute[] = [];
    for (let i = 0; i < KEEP_MIN; i++) record(W, h, T0 + i * MIN, [0], i < 6 ? ['18'] : []);
    const a = activity(W, h, T0 + (KEEP_MIN - 1) * MIN);
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

describe('every board', () => {
  it.each(BOARDS.map((b) => [b.id, b] as const))('%s: each tile is a channel, each channel a tile or a twin, each side fits a capture', (_, b) => {
    const tiles = b.strips.flatMap((s) => s.keys);
    expect(new Set(tiles).size).toBe(tiles.length);
    for (const k of tiles) expect(b.channels.some((c) => c.key === k && !c.second), k).toBe(true);
    for (const c of b.channels) {
      if (c.second) expect(b.channels.find((t) => t.key === c.twin)?.twin).toBe(c.key);
      else expect(tiles, c.key).toContain(c.key);
      expect(c.use, c.key).toBeTruthy();
      expect(channelOn(b, c.hz)?.key).toBe(c.key);
    }
    for (const [lo, hi] of b.sides) expect(hi - lo).toBeLessThan(2_160_000 - 2 * b.reachHz);
  });
});

describe('marine and CB', () => {
  const M = boardFor('marine')!;
  const C = boardFor('cb')!;
  it('splits a duplex channel into a shore and a ship side', () => {
    const shore = channelOn(M, 161_800_000)!;
    const ship = channelOn(M, 157_200_000)!;
    expect([shore.name, ship.name, shore.twin, ship.twin]).toEqual(['24 shore', '24 ship', '24S', '24']);
    expect(sideOf(M, shore.hz)).toBe(1);
    expect(sideOf(M, ship.hz)).toBe(0);
    expect(sideOf(M, 159e6)).toBe(0);
  });
  it('records a shore channel only while the shore side is heard', () => {
    const h: Minute[] = [];
    record(M, h, T0, [0], ['16', '24']);
    expect(h[0].on).toEqual(['16']);
  });
  it('puts distress first, and describes it', () => {
    expect(M.strips[0].keys[0]).toBe('16');
    expect(channelOn(M, 156_800_000)?.word).toBe('Distress');
    expect(stripOf(M, '22A')?.name).toBe('Safety and Coast Guard');
  });
  it('names CB 9 and 19, and tunes the sideband channels in sideband', () => {
    expect(channelOn(C, 27_065_000)?.word).toBe('Emergency');
    expect(channelOn(C, 27_185_000)?.word).toBe('Highway');
    expect(channelOn(C, 27_365_000)?.mode).toBe('usb');
    expect(channelOn(C, 27_385_000)?.mode).toBe('lsb');
    expect(channelOn(C, 27_065_000)?.mode).toBeUndefined();
  });
  it('takes old walkie band ids', () => {
    expect(boardFor('gmrs')).toBe(W);
    expect(boardFor('rail')).toBeUndefined();
  });
});
