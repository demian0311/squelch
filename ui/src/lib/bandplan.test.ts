import { describe, expect, it } from 'vitest';
import { BAND_PLANS, segmentAt, segmentTarget, segmentsIn } from './bandplan';
import { modeAt } from './radio';
import type { BandPlan } from './api';

describe('band plans', () => {
  it('keeps every plan in order, inside its band, without overlaps', () => {
    const edges: Record<string, [number, number]> = { '10m': [28e6, 29.7e6], '6m': [50e6, 54e6], '2m': [144e6, 148e6], '70cm': [420e6, 450e6] };
    for (const [id, segs] of Object.entries(BAND_PLANS)) {
      const [lo, hi] = edges[id];
      segs.forEach((s, i) => {
        expect(s.lo).toBeLessThan(s.hi);
        expect(s.lo).toBeGreaterThanOrEqual(lo);
        expect(s.hi).toBeLessThanOrEqual(hi);
        if (i) expect(s.lo).toBeGreaterThanOrEqual(segs[i - 1].hi);
      });
    }
  });
  it('finds the segment under a frequency', () => {
    expect(segmentAt('2m', 144_200_000)?.mode).toBe('usb');
    expect(segmentAt('2m', 146_940_000)?.use).toBe('rout');
    expect(segmentAt('6m', 51_990_000)).toBeUndefined();
    expect(segmentAt('fm', 101_100_000)).toBeUndefined();
    expect(segmentAt(undefined, 144_200_000)).toBeUndefined();
  });
  it('clips segments to the view', () => {
    const in2m = segmentsIn('2m', 145_000_000, 146_000_000);
    expect(in2m[0]).toMatchObject({ name: 'Simplex, packet', left: 0 });
    expect(in2m[0].width).toBeCloseTo(10);
    expect(in2m.at(-1)!.left + in2m.at(-1)!.width).toBeCloseTo(100);
    expect(segmentsIn('fm', 88e6, 90e6)).toEqual([]);
  });
  it('tunes the middle of what shows of a segment', () => {
    const s = segmentAt('70cm', 443e6)!;
    expect(segmentTarget(s, 420e6, 450e6)).toBe(443_500_000);
    expect(segmentTarget(s, 443e6, 446e6)).toBe(444_000_000);
  });
  it('gives the segment mode when nothing named says otherwise', () => {
    const plan = { groups: [], bands: [{ id: '2m', name: '2 m', group: 'ham', low_mhz: 144, high_mhz: 148, mode: 'nfm' }], bookmarks: [] } as unknown as BandPlan;
    expect(modeAt(plan, [], 144_150_000, 'wfm')).toBe('usb');
    expect(modeAt(plan, [], 146_520_000, 'wfm')).toBe('nfm');
    expect(modeAt(plan, [{ mhz: 144.15, name: 'x', mode: 'am' } as never], 144_150_000, 'wfm')).toBe('am');
  });
});
