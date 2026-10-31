import { describe, expect, it } from 'vitest';
import { callAge, callBlock, p25Site, p25Summary } from './p25';
import type { P25, P25Call } from './api';

const call: P25Call = { group: 9100, source: 352783, hz: 769_418_750, encrypted: false, tdma: false, emergency: false, at_secs: 100 };
const base: P25 = {
  state: 'control', now_secs: 110, nac: 0xd17, wacn: 0xbee07, system: 0xd1c, rfss: 3, site: 55, control_hz: 773_793_750,
  ppm: -56.5, messages_per_sec: 24, call: null, recent: [], voice_error: null, voice_errors_pct: 0,
};

describe('p25Summary', () => {
  it('names the call being played', () => {
    expect(p25Summary({ ...base, state: 'voice', call })).toBe('Playing talkgroup 9100');
  });
  it('names the site while waiting', () => {
    expect(p25Summary(base)).toBe('Site 3-55 · system D1C · waiting for a call');
  });
  it('says it is searching before a lock', () => {
    expect(p25Summary({ ...base, state: 'searching', rfss: null, site: null })).toBe('Looking for a control channel…');
  });
});

describe('p25Site', () => {
  it('waits for the site message', () => {
    expect(p25Site({ ...base, site: null })).toBe(null);
  });
});

describe('callBlock', () => {
  it('explains calls that cannot play', () => {
    expect(callBlock({ ...call, encrypted: true }, null)).toBe('Encrypted');
    expect(callBlock({ ...call, tdma: true }, null)).toBe('Phase 2, not supported yet');
    expect(callBlock(call, 'P25 voice needs mbelib')).toBe('Voice decoder missing');
    expect(callBlock(call, null)).toBe(null);
  });
});

describe('callAge', () => {
  it('counts seconds, then minutes', () => {
    expect(callAge(call, 112)).toBe('12 s ago');
    expect(callAge(call, 280)).toBe('3 min ago');
  });
});
