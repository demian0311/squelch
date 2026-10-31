import { describe, expect, it } from 'vitest';
import { hdState, nextHd, trafficGrid } from './hd';
import type { HdImage } from './api';

describe('nextHd', () => {
  it('cycles off → HD1 → HD2 → off', () => {
    expect(nextHd(null, [0, 1])).toBe(0);
    expect(nextHd(0, [0, 1])).toBe(1);
    expect(nextHd(1, [0, 1])).toBe(null);
  });
  it('offers HD1 before any program is known', () => {
    expect(nextHd(null, [])).toBe(0);
    expect(nextHd(0, [])).toBe(null);
  });
});

describe('trafficGrid', () => {
  const tile = (row: number, col: number): HdImage => ({ kind: 'traffic', name: '', row, col, data_url: `${row}${col}` });
  it('places tiles row-major and leaves gaps empty', () => {
    const g = trafficGrid([tile(1, 1), tile(3, 3), { kind: 'logo', name: '', row: 0, col: 0, data_url: 'x' }])!;
    expect(g[0]).toBe('11');
    expect(g[8]).toBe('33');
    expect(g.filter(Boolean)).toHaveLength(2);
  });
  it('is null without tiles', () => {
    expect(trafficGrid([])).toBeNull();
  });
});

describe('hdState', () => {
  it('reports the furthest stage reached', () => {
    expect(hdState({ error: null, synced: false, digital: false })).toMatch(/Looking/);
    expect(hdState({ error: null, synced: true, digital: true })).toBe('Playing digital audio');
    expect(hdState({ error: 'no lib', synced: false, digital: false })).toBe('no lib');
  });
});
