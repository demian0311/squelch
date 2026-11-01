import { describe, expect, it } from 'vitest';
import { groupHue, resolve, waterfallStops } from './theme';

const t = resolve({});

describe('groupHue', () => {
  it('gives the built-in groups their own colours', () => {
    expect(groupHue(t, 'listen', [])).toBe(t.blue);
    expect(groupHue(t, 'ham', [])).toBe(t.green);
    expect(groupHue(t, 'space', [])).toBe(t.magenta);
  });
  it('gives unknown groups spare colours in order', () => {
    const ids = ['listen', 'mine', 'yours'];
    expect(groupHue(t, 'mine', ids)).toBe(t.cyan);
    expect(groupHue(t, 'yours', ids)).toBe(t.yellow);
  });
  it('falls back to the accent with no group', () => {
    expect(groupHue(t, undefined, [])).toBe(t.accent);
  });
});

describe('waterfallStops', () => {
  it('starts dark and ends on the brightest foreground', () => {
    const s = waterfallStops(t);
    expect(s[0]).toBe(t.darker_background);
    expect(s[s.length - 1]).toBe(t.bright_foreground);
  });
});
