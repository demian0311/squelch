import { describe, expect, it } from 'vitest';
import { NO_PICKS, barPx, folded, groupRows, holdSay, p25Site, p25Summary, sinceSay, systemKey } from './p25';
import type { P25, P25Call, P25Talkgroup } from './api';

const call: P25Call = { group: 9100, source: 352783, hz: 769_418_750, encrypted: false, tdma: false, emergency: false, at_secs: 100 };
const base: P25 = {
  state: 'control', now_secs: 110, nac: 0xd17, wacn: 0xbee07, system: 0xd1c, rfss: 3, site: 55, control_hz: 773_793_750,
  ppm: -56.5, messages_per_sec: 24, call: null, groups: [], voice_error: null, voice_errors_pct: 0, scan: null, trying_hz: null, tried: [],
};

describe('p25Summary', () => {
  it('names the call being played', () => {
    expect(p25Summary({ ...base, state: 'voice', call })).toBe('Playing talkgroup 9100');
  });
  it('names the site while waiting', () => {
    expect(p25Summary(base)).toBe('Site 3-55 · system D1C · waiting for a call');
  });
  it('says it is searching before a lock', () => {
    expect(p25Summary({ ...base, state: 'searching' })).toBe('Looking for a control channel in this capture…');
  });
});

describe('p25Site', () => {
  it('waits for the site message', () => {
    expect(p25Site({ ...base, site: null })).toBe(null);
  });
});

describe('scanSay', () => {
  it('says which hop of how many and where', () => {
    const p: P25 = { ...base, state: 'searching', rfss: null, site: null, scan: { hop: 3, hops: 14, lo_hz: 854.6e6, hi_hz: 856.4e6 } };
    expect(p25Summary(p)).toBe('Scanning for a control channel · hop 3 of 14 · 854.600–856.400 MHz');
    expect(p25Summary({ ...p, scan: null })).toBe('Looking for a control channel in this capture…');
  });
});

const tg = (group: number, calls: number, last_secs: number, more: Partial<P25Talkgroup> = {}): P25Talkgroup => ({
  group, calls, last_secs, hz: 858_212_500, source: 0, encrypted: false, tdma: false, emergency: false, minutes: [0, 0, 0, 0, 0, 0, 0, 0, 0, calls], ...more,
});

describe('groupRows', () => {
  const groups = [tg(2024, 7, 100), tg(2110, 9, 90), tg(2001, 3, 105), tg(4000, 20, 108, { encrypted: true }), tg(4100, 2, 108, { tdma: true })];
  it('keeps only talkgroups that can play, busiest first', () => {
    expect(groupRows(groups, NO_PICKS).map((g) => g.group)).toEqual([2110, 2024, 2001]);
  });
  it('puts the held talkgroup first and skipped ones last', () => {
    expect(groupRows(groups, { hold: 2001, skip: [2110] }).map((g) => g.group)).toEqual([2001, 2024, 2110]);
  });
  it('folds what will never play into counts', () => {
    expect(folded(groups)).toEqual({ encrypted: '20 encrypted calls on 1 talkgroup', tdma: '2 Phase 2 calls on 1 talkgroup' });
    expect(folded(groups.slice(0, 3))).toEqual({ encrypted: null, tdma: null });
  });
});

describe('picks', () => {
  it('are kept by system id in hex', () => {
    expect(systemKey(base)).toBe('d1c');
    expect(systemKey({ ...base, system: null })).toBe(null);
  });
  it('explain a hold that keeps the receiver quiet', () => {
    expect(holdSay({ ...base, groups: [tg(2024, 3, 50)] }, 2024)).toBe('Holding talkgroup 2024 · last call 1 min ago');
    expect(holdSay(base, 2024)).toBe('Holding talkgroup 2024 · no call yet');
  });
});

describe('bars', () => {
  it('give a quiet minute a stub and scale the rest to the busiest', () => {
    expect(barPx(0, 5)).toBe(2);
    expect(barPx(5, 5)).toBe(14);
    expect(barPx(1, 5)).toBe(5);
  });
  it('count seconds, then minutes', () => {
    expect(sinceSay(12)).toBe('12 s ago');
    expect(sinceSay(170)).toBe('3 min ago');
  });
});
