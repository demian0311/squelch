import { describe, expect, it } from 'vitest';
import { encodeOffset } from './gl';

describe('encodeOffset', () => {
  it('round-trips the offsets the shader decodes', () => {
    for (const bins of [-32000, -1, 0, 1, 2047.4, 31999]) {
      const [hi, lo] = encodeOffset(bins);
      expect(hi * 256 + lo - 32768).toBe(Math.round(bins));
    }
  });
});
