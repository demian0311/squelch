import { describe, expect, it } from 'vitest';
import type { BandPlan } from './api';
import { addLine, cellsToPixels, emsRanges, oneEms, railId, stripHz, stripPick, stripSay, stripSections, stripX, STRIP_ROWS, type LookLine } from './ems';

const band = (id: string, low: number, high: number) => ({ id, group: 'safety', name: id, low_mhz: low, high_mhz: high, start_mhz: low, mode: 'p25' }) as BandPlan['bands'][number];
const plan = { groups: [{ id: 'safety', name: 'Public safety' }], bands: [band('fm', 88, 108), band('ps700', 769, 775), band('ps800', 851, 869)] } as unknown as BandPlan;
const line = (seq: number, cells: Record<number, number> = {}): LookLine => {
  const c = new Array(1440).fill(30);
  for (const [i, v] of Object.entries(cells)) c[+i] = v;
  return { seq, parts: [{ lo_hz: 851e6, hi_hz: 869e6, cells: c }] };
};

describe('one EMS entry', () => {
  it('shows one entry and leaves others alone', () => {
    expect(oneEms(plan).bands.map((b) => [b.id, b.name])).toEqual([['fm', 'fm'], ['ps700', 'EMS']]);
  });
  it('still shows one when only one band is left', () => {
    const only = { ...plan, bands: [band('ps800', 851, 869)] };
    expect(oneEms(only).bands.map((b) => [b.id, b.name])).toEqual([['ps800', 'EMS']]);
  });
  it('marks the one entry whichever band is tuned', () => {
    expect(railId(plan, 'ps800')).toBe('ps700');
    expect(railId(plan, 'fm')).toBe('fm');
    expect(railId(plan, null)).toBeNull();
  });
  it('checks both ranges only on an EMS band', () => {
    expect(emsRanges(plan, plan.bands[2])).toEqual([[769e6, 775e6], [851e6, 869e6]]);
    expect(emsRanges(plan, plan.bands[0])).toBeNull();
  });
});

describe('the strip', () => {
  const s = stripSections([[769e6, 775e6], [851e6, 869e6]], 970);
  it('sizes the bands by MHz with the gap closed', () => {
    expect(s[0].w).toBeCloseTo(240);
    expect(s[1].x).toBeCloseTo(250);
    expect(s[1].w).toBeCloseTo(720);
  });
  it('turns pixels into frequencies and back', () => {
    expect(stripX(s, 860e6)).toBeCloseTo(610);
    expect(stripHz(s, 610)).toBeCloseTo(860e6);
  });
  it('has no frequency in the gap', () => {
    expect(stripHz(s, 245)).toBeNull();
    expect(stripX(s, 800e6)).toBeNull();
  });
  it('keeps a one-cell signal when squeezing cells into pixels', () => {
    const cells = new Array(1440).fill(30);
    cells[777] = 200;
    const px = cellsToPixels(cells, 720);
    expect(px.length).toBe(720);
    expect(px[388]).toBe(200);
    expect(px.filter((v) => v === 200).length).toBe(1);
  });
  it('stretches cells across more pixels than there are cells', () => {
    expect([...cellsToPixels([10, 200], 4)]).toEqual([10, 10, 200, 200]);
  });
  it('snaps a click to a signal nearby, and otherwise leaves it', () => {
    const lines = [line(1, { 235: 180 })];
    const atCell = 851e6 + 235.5 * 12_500;
    expect(stripPick(lines, atCell + 30_000, 25_000)).toBeCloseTo(atCell);
    expect(stripPick(lines, 860e6, 25_000)).toBe(860e6);
  });
  it('snaps to a signal seen in an older line', () => {
    expect(stripPick([line(2), line(1, { 235: 180 })], 851e6 + 235.5 * 12_500, 25_000)).toBeCloseTo(851e6 + 235.5 * 12_500);
  });
  it('adds a line once, newest first, up to the limit', () => {
    let lines: LookLine[] = [];
    lines = addLine(lines, line(1));
    lines = addLine(lines, line(1));
    expect(lines.length).toBe(1);
    for (let i = 2; i < STRIP_ROWS + 5; i++) lines = addLine(lines, line(i));
    expect(lines.length).toBe(STRIP_ROWS);
    expect(lines[0].seq).toBe(STRIP_ROWS + 4);
    expect(addLine(lines, null)).toBe(lines);
  });
  it('says what it is doing', () => {
    expect(stripSay(true, 3, 2)).toMatch(/paused/);
    expect(stripSay(false, 0, null)).toMatch(/first check/);
    expect(stripSay(false, 2, 12)).toMatch(/12 s ago/);
    expect(stripSay(false, 2, 120)).toMatch(/2 min ago/);
  });
});
