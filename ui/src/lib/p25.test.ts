import { describe, expect, it } from 'vitest';
import { callAge, callGlyph, p25Site, p25Summary } from './p25';
import type { P25, P25Call } from './api';

const call: P25Call = { group: 9100, source: 352783, hz: 769_418_750, encrypted: false, tdma: false, emergency: false, at_secs: 100 };
const base: P25 = {
  state: 'control', now_secs: 110, nac: 0xd17, wacn: 0xbee07, system: 0xd1c, rfss: 3, site: 55, control_hz: 773_793_750,
  ppm: -56.5, messages_per_sec: 24, call: null, recent: [], voice_error: null, voice_errors_pct: 0, scan: null, trying_hz: null, tried: [],
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

describe('callGlyph', () => {
  it('marks calls that will never play with a padlock or a muted speaker', () => {
    expect(callGlyph({ ...call, encrypted: true })?.icon).toBe('lock');
    expect(callGlyph({ ...call, tdma: true })?.icon).toBe('mute');
    expect(callGlyph(call)).toBe(null);
  });
});

describe('callAge', () => {
  it('counts seconds, then minutes', () => {
    expect(callAge(call, 112)).toBe('12 s ago');
    expect(callAge(call, 280)).toBe('3 min ago');
  });
});
