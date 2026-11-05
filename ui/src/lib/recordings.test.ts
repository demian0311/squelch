import { describe, expect, it } from 'vitest';
import type { SavedRecording } from './api';
import { byDay, clockLength, dayLabel, nextAfter, parseRecordingName, sizeLabel } from './recordings';

const rec = (at: number, file = `f${at}`): SavedRecording => ({ file, at, hz: 101.1e6, mode: 'wfm', secs: 10, bytes: 40_000 });

describe('parseRecordingName', () => {
  it('reads the recorder\'s name back', () => {
    expect(parseRecordingName('/home/x/Music/squelch/2026-11-01T153012Z 101.100MHz wfm.opus')).toEqual({ at: 1_793_547_012, hz: 101_100_000, mode: 'wfm' });
    expect(parseRecordingName('/home/x/notes.txt')).toBeNull();
  });
});

describe('labels', () => {
  it('prints lengths and sizes', () => {
    expect(clockLength(133.4)).toBe('2:13');
    expect(clockLength(3_725)).toBe('1:02:05');
    expect(sizeLabel(3_100_000)).toBe('3.1 MB');
    expect(sizeLabel(40_000)).toBe('40 kB');
  });
  it('names days from the viewer\'s clock', () => {
    const now = new Date(2026, 10, 5, 14, 0).getTime() / 1000;
    expect(dayLabel(now - 3600, now)).toBe('Today');
    expect(dayLabel(new Date(2026, 10, 4, 23, 59).getTime() / 1000, now)).toBe('Yesterday');
    expect(dayLabel(new Date(2026, 10, 2, 3, 3).getTime() / 1000, now)).toMatch(/^Monday, (2 November|November 2)$/);
  });
});

describe('byDay', () => {
  it('groups newest first within local days', () => {
    const now = new Date(2026, 10, 5, 14, 0).getTime() / 1000;
    const a = rec(new Date(2026, 10, 5, 9, 0).getTime() / 1000);
    const b = rec(new Date(2026, 10, 5, 13, 0).getTime() / 1000);
    const c = rec(new Date(2026, 10, 3, 7, 0).getTime() / 1000);
    const days = byDay([a, c, b], now);
    expect(days.map((d) => d.label)).toEqual(['Today', expect.stringMatching(/3/)]);
    expect(days[0].rows).toEqual([b, a]);
    expect(nextAfter([a, c, b], b.file)).toBe(a);
    expect(nextAfter([a, c, b], c.file)).toBeNull();
  });
});
