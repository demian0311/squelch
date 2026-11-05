import { describe, expect, it } from 'vitest';
import type { Dmr, DmrCall } from './api';
import { agoSay, callSay, callState, dmrSummary, lengthSay } from './dmr';

const call = (over: Partial<DmrCall> = {}): DmrCall => ({ slot: 1, group: true, dst: 31088, src: 3104321, emergency: false, encrypted: false, start_secs: 10, last_secs: 22, frames: 60, bad_frames: 2, ...over });
const dmr = (over: Partial<Dmr> = {}): Dmr => ({
  now_secs: 100,
  heard: true,
  source: 'repeater',
  colour_code: 1,
  slots: [
    { call: null, playing: false, calls: 0 },
    { call: null, playing: false, calls: 0 },
  ],
  recent: [],
  voice_error: null,
  bursts_per_sec: 33,
  encrypted_calls: 0,
  ...over,
});

describe('DMR words', () => {
  it('says who calls whom, and owns up to what it has not read yet', () => {
    expect(callSay(call())).toBe('Talkgroup 31088 ← radio 3104321');
    expect(callSay(call({ group: false, dst: 12 }))).toBe('Radio 12, private ← radio 3104321');
    expect(callSay(call({ dst: null, src: null }))).toBe('Reading who is calling…');
  });
  it('summarises the playing call first, then the quiet channel', () => {
    const d = dmr({ slots: [{ call: call(), playing: true, calls: 1 }, { call: call({ slot: 2, dst: 9, encrypted: true }), playing: false, calls: 0 }] });
    expect(dmrSummary(d)).toBe('Talkgroup 31088 ← radio 3104321 · slot 1');
    expect(callState(d, 0)).toBe('playing');
    expect(callState(d, 1)).toBe('encrypted · muted');
    expect(dmrSummary(dmr())).toBe('DMR heard · colour code 1 · waiting for a call');
    expect(dmrSummary(dmr({ heard: false, colour_code: null }))).toBe('No DMR heard here yet');
  });
  it('tells lengths and ages in plain words', () => {
    expect(lengthSay(call())).toBe('12 s');
    expect(lengthSay(call({ start_secs: 0, last_secs: 65 }))).toBe('1:05');
    expect(agoSay(100, call())).toBe('1 min ago');
    expect(agoSay(24, call())).toBe('just now');
  });
});
