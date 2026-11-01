import { describe, expect, it } from 'vitest';
import type { Rds } from './api';
import { MAX_AGE_MS, TOUCH_MS, noteHeard, pickTick, stripLabels, stripTicks, type Heard } from './stations';

const rds = (ps: string): Rds => ({ ps, call: null, radiotext: null, pty: null, title: null, artist: null }) as unknown as Rds;
const sig = (mhz: number, snr_db = 20) => ({ hz: mhz * 1e6, snr_db });

describe('noteHeard', () => {
  it('records stations on the FM grid, named from RDS, and skips weak or off-grid signals', () => {
    const out = noteHeard([], [sig(101.102), sig(98.5, 3), sig(99.45)], new Map([[101.1e6, rds('KOSI')]]), 1000);
    expect(out).toEqual([{ hz: 101.1e6, at: 1000, snrDb: 20, name: 'KOSI' }]);
  });

  it('returns the same list while a station was stamped recently', () => {
    const list: Heard[] = [{ hz: 101.1e6, at: 1000, snrDb: 20, name: 'KOSI' }];
    expect(noteHeard(list, [sig(101.1)], new Map(), 1000 + TOUCH_MS - 1)).toBe(list);
    expect(noteHeard(list, [sig(101.1)], new Map(), 1000 + TOUCH_MS)[0].at).toBe(1000 + TOUCH_MS);
  });

  it('keeps an old name when RDS is not in yet, and takes a new one at once', () => {
    const list: Heard[] = [{ hz: 101.1e6, at: 1000, snrDb: 20, name: 'KOSI' }];
    expect(noteHeard(list, [sig(101.1)], new Map(), 1000 + TOUCH_MS)[0].name).toBe('KOSI');
    expect(noteHeard(list, [sig(101.1)], new Map([[101.1e6, rds('KOSI 101')]]), 1001)[0].name).toBe('KOSI 101');
  });

  it('forgets stations not heard for MAX_AGE_MS', () => {
    const list: Heard[] = [{ hz: 88.1e6, at: 0, snrDb: 10 }];
    expect(noteHeard(list, [], new Map(), MAX_AGE_MS + 1)).toEqual([]);
  });
});

describe('stripTicks', () => {
  it('marks this run as fresh and merges a bookmark into its station', () => {
    const heard: Heard[] = [
      { hz: 97.3e6, at: 50, snrDb: 30, name: 'KBCO' },
      { hz: 101.1e6, at: 200, snrDb: 20 },
    ];
    const t = stripTicks(heard, [{ mhz: 101.1, name: 'Mine' }, { mhz: 104.3, name: 'Later' }, { mhz: 150, name: 'Off band' }], 100, 87.8e6, 108e6);
    expect(t.map((x) => [x.title, x.fresh, x.saved])).toEqual([
      ['97.3 KBCO', false, false],
      ['101.1 Mine', true, true],
      ['104.3 Later', false, true],
    ]);
  });
});

describe('pickTick', () => {
  const ticks = stripTicks([{ hz: 97.3e6, at: 0, snrDb: 30 }, { hz: 97.5e6, at: 0, snrDb: 30 }], [], 0, 87.8e6, 108e6);
  it('takes the nearest tick within reach, else nothing', () => {
    expect(pickTick(ticks, 97.42e6, 20_000)?.hz).toBe(97.5e6);
    expect(pickTick(ticks, 99e6, 20_000)).toBeNull();
  });
});

describe('stripLabels', () => {
  it('spaces round MHz labels by at least the minimum', () => {
    expect(stripLabels(87.8e6, 108e6, 1000)).toEqual([88, 90, 92, 94, 96, 98, 100, 102, 104, 106, 108].map((m) => m * 1e6));
  });
});
