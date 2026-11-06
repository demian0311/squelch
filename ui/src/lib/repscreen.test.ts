import { describe, expect, it } from 'vitest';
import type { DmrCall, DmrSlot, RepeaterScan } from './api';
import type { RepeaterPoint } from './repeaters';
import { REMEMBER_MS, bestCentre, rememberHeard, repRows, scanChannels, shortAgo } from './repscreen';

const pt = (hz: number, call: string, digital: string | null = null, reach = true): RepeaterPoint => ({ id: call, hz, call, city: 'Miami', x: 0, y: 0, km: reach ? 3 : 60, reach, digital });
const points = [pt(443_075_000, 'KZ4TI', 'DMR'), pt(443_550_000, 'WA4SRL'), pt(444_900_000, 'W2GGI', 'DMR'), pt(444_900_000, 'K7HJE', 'DMR'), pt(446_000_000, 'W4ABC', 'D-STAR'), pt(449_000_000, 'N2GKG', null, false)];
const slot = (call: DmrCall | null, playing: boolean): DmrSlot => ({ call, playing, calls: 0 });
const emptySlot = slot(null, false);
const dmrInfo = (slots: [DmrSlot, DmrSlot]) => ({ now_secs: 10, heard: true, source: 'repeater' as const, colour_code: 1, slots, recent: [], voice_error: null, bursts_per_sec: 33, encrypted_calls: 0 });
const call: DmrCall = { slot: 2, group: true, dst: 31088, src: 3104321, emergency: false, encrypted: false, start_secs: 0, last_secs: 10, frames: 100, bad_frames: 2 };

describe('scanChannels', () => {
  it('watches what the radio can play, once per frequency', () => {
    expect(scanChannels(points)).toEqual([
      { hz: 443_075_000, dmr: true },
      { hz: 443_550_000, dmr: false },
      { hz: 444_900_000, dmr: true },
      { hz: 449_000_000, dmr: false },
    ]);
  });
});

describe('bestCentre', () => {
  it('covers the most repeaters, DMR first, and stays off every channel', () => {
    const c = bestCentre(points, 2_400_000)!;
    for (const hz of [443_075_000, 443_550_000, 444_900_000]) expect(Math.abs(hz - c)).toBeLessThanOrEqual(1_050_000);
    for (const ch of scanChannels(points)) expect(Math.abs(ch.hz - c)).toBeGreaterThanOrEqual(20_000);
  });
  it('keeps a clicked row inside, even alone', () => {
    const c = bestCentre(points, 2_400_000, 449_000_000)!;
    expect(Math.abs(449_000_000 - c)).toBeLessThanOrEqual(1_050_000);
    expect(bestCentre([], 2_400_000, 1)).toBe(1);
  });
});

describe('repRows', () => {
  const scan: RepeaterScan = {
    now_secs: 10,
    playing: 443_075_000,
    hold: null,
    channels: [
      { hz: 443_075_000, dmr: dmrInfo([emptySlot, slot(call, true)]), on_air: false, heard: true, last_on_secs: 10 },
      { hz: 443_550_000, dmr: null, on_air: true, heard: true, last_on_secs: 10 },
      { hz: 444_900_000, dmr: dmrInfo([emptySlot, emptySlot]), on_air: false, heard: true, last_on_secs: null },
      { hz: 449_000_000, dmr: null, on_air: false, heard: false, last_on_secs: null },
    ],
  };
  const now = 1_000_000_000_000;
  const memory = { '444900000': now - 6 * 60_000 };
  it('says what each repeater does and leaves out what the radio cannot decode', () => {
    const rows = repRows(points, scan, memory, now);
    expect(rows.map((r) => r.p.call)).toEqual(['KZ4TI', 'WA4SRL', 'W2GGI', 'K7HJE', 'N2GKG']);
    expect(rows[0]).toMatchObject({ dmr: true, heard: true, state: 'playing', say: 'Talking · playing' });
    expect(rows[0].detail).toBe('Talkgroup 31088 ← radio 3104321 · 10 s · slot 2');
    expect(rows[1]).toMatchObject({ dmr: false, state: 'talking', say: 'On air', detail: 'a carrier now · click to hear it' });
    expect(rows[2]).toMatchObject({ state: 'quiet', say: 'Quiet', detail: 'last call 6 min ago' });
    expect(rows[4]).toMatchObject({ heard: false, state: 'away', say: 'Out of the capture' });
    expect(rows[4].detail).toBe('click to move there · nothing heard in 7 days');
  });
  it('shows the level, a carrier under the mark, and the facts as numbers', () => {
    const levels = new Map([
      [443_075_000, 31],
      [443_550_000, 24],
      [444_900_000, 6],
    ]);
    const rows = repRows(points, scan, memory, now, levels);
    expect(rows[0].snr).toBe(31);
    expect(rows[0].facts).toMatchObject({ to: 'Talkgroup 31088 ← radio 3104321', length: '10 s', lastSecs: 0 });
    expect(rows[0].facts.slots).toEqual([
      { call: false, encrypted: false },
      { call: true, encrypted: false },
    ]);
    expect(rows[1]).toMatchObject({ state: 'talking', snr: 24 });
    // a DMR output with a carrier but no call stays quiet: the level still shows
    expect(rows[2]).toMatchObject({ state: 'quiet', snr: 6 });
    expect(rows[2].facts.lastSecs).toBe(360);
    expect(rows[4]).toMatchObject({ state: 'away', snr: null });
    expect(rows[4].facts.lastSecs).toBeNull();
    // an FM carrier the engine judged under the squelch
    const under = repRows(points, { ...scan, channels: scan.channels.map((c) => (c.hz === 443_550_000 ? { ...c, on_air: false } : c)) }, memory, now, levels);
    expect(under[1]).toMatchObject({ state: 'under', snr: 24, say: 'Carrier under the mark' });
    expect(repRows(points, { ...scan, channels: scan.channels.map((c) => (c.hz === 443_550_000 ? { ...c, on_air: false } : c)) }, memory, now)[1].state).toBe('quiet');
  });
  it('shortAgo keeps to one unit', () => {
    expect([shortAgo(40), shortAgo(360), shortAgo(7200), shortAgo(3 * 86_400)]).toEqual(['40 s', '6 min', '2 h', '3 d']);
  });
});

describe('rememberHeard', () => {
  const now = 1_000_000_000_000;
  const scan: RepeaterScan = { now_secs: 100, playing: null, hold: null, channels: [{ hz: 1, dmr: null, on_air: true, heard: true, last_on_secs: 99 }, { hz: 2, dmr: null, on_air: false, heard: true, last_on_secs: 50 }] };
  it('notes activity now, at most every few seconds, and forgets after a week', () => {
    expect(rememberHeard({}, scan, now)).toEqual({ '1': now });
    expect(rememberHeard({ '1': now - 1000 }, scan, now)).toBeNull();
    expect(rememberHeard({ '1': now - 6000, '9': now - REMEMBER_MS - 1 }, scan, now)).toEqual({ '1': now });
    expect(rememberHeard({ '1': now }, null, now)).toBeNull();
  });
});
