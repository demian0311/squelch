import { describe, expect, it } from 'vitest';
import type { Rds } from './api';
import { AM_SLACK_HZ, MAX_AGE_MS, TOUCH_MS, HOLD_DB, heardType, noteHeard, pickTick, stripLabels, stripTicks, withHeard, dialRows, sortDial, holdSignals, localStations, localName, localType, heardName, type Heard } from './stations';
import type { FmStation } from './api';

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

  it('remembers what a station plays, through a gap in RDS and across a change', () => {
    const list = noteHeard([], [sig(101.1)], new Map([[101.1e6, { ...rds('KOSI'), pty: 'News' }]]), 1000);
    expect(heardType(list, 101.102e6)).toBe('News');
    expect(heardType(noteHeard(list, [sig(101.1)], new Map(), 1000 + TOUCH_MS), 101.1e6)).toBe('News');
    expect(heardType(noteHeard(list, [sig(101.1)], new Map([[101.1e6, { ...rds('KOSI'), pty: 'Talk' }]]), 1001), 101.1e6)).toBe('Talk');
    expect(heardType(list, 99.5e6)).toBeUndefined();
  });

  it('names cards from earlier runs, but never over RDS or a bookmark', () => {
    const list: Heard[] = [{ hz: 101.1e6, at: 1000, snrDb: 20, name: 'KOSI' }];
    const at = (hz: number) => heardName(list, hz);
    const card = { hz: 101.102e6, widthHz: 0, snrDb: 20, title: 'FM 101.1', sub: 'Wide FM', tag: 'HEARD', saved: false, byFreq: true };
    expect(withHeard([card], at)[0].title).toBe('KOSI 101.1');
    expect(withHeard([{ ...card, byFreq: false, title: 'KBPI 101.1' }], at)[0].title).toBe('KBPI 101.1');
    expect(withHeard([{ ...card, saved: true, title: 'Mine' }], at)[0].title).toBe('Mine');
    expect(withHeard([{ ...card, hz: 99.5e6 }], at)[0].title).toBe('FM 101.1');
  });

  it('forgets stations not heard for MAX_AGE_MS', () => {
    const list: Heard[] = [{ hz: 88.1e6, at: 0, snrDb: 10 }];
    expect(noteHeard(list, [], new Map(), MAX_AGE_MS + 1)).toEqual([]);
  });
});

describe('stripTicks', () => {
  it('keeps AM channels 10 kHz apart and prints kHz precision below 30 MHz', () => {
    const t = stripTicks([], [{ mhz: 0.85, name: 'KOA' }, { mhz: 0.86, name: 'Next' }, { mhz: 10, name: 'WWV time signal' }], 0, 0.53e6, 13.9e6, AM_SLACK_HZ);
    expect(t.map((x) => x.title)).toEqual(['0.850 KOA', '0.860 Next', '10.000 WWV time signal']);
  });
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

describe('the dial', () => {
  const st = (mhz: number, call: string, km: number, in_reach = true): FmStation => ({ hz: mhz * 1e6, call, city: 'Miami', state: 'FL', km, in_reach });
  const old = (mhz: number, name: string | undefined, snrDb = 20, at = 1): Heard => ({ hz: mhz * 1e6, at, snrDb, name, type: 'Country' });

  it('takes the nearest station in reach on a shared channel, unless the radio named another', () => {
    const list = [st(88.1, 'WRGP', 58), st(88.1, 'WAYF', 66)];
    expect(localStations(list, [], new Map()).get(88.1e6)?.call).toBe('WRGP');
    expect(localStations(list, [old(88.1, 'WAYF')], new Map()).get(88.1e6)?.call).toBe('WAYF');
  });

  it('lists a station past its reach only once heard', () => {
    const list = [st(93.5, 'WZFL', 110, false)];
    expect(localStations(list, [], new Map()).size).toBe(0);
    expect(localStations(list, [old(93.5, 'WZFL')], new Map()).get(93.5e6)?.call).toBe('WZFL');
  });

  it('ignores a name and type remembered from somewhere else', () => {
    const heard = [old(94.9, 'KDHM'), old(99.5, 'KQMT')];
    const local = localStations([st(94.9, 'WZTU', 4)], heard, new Map());
    expect(localName(true, local, heard, 94.9e6)).toBe('WZTU');
    expect(localType(true, local, heard, 94.9e6)).toBeUndefined();
    expect(localName(true, local, heard, 99.5e6)).toBeUndefined();
    const rows = dialRows(true, local, heard, [], new Map(), 1000);
    expect(rows.map((r) => r.title)).toEqual(['WZTU']);
    expect(rows[0].snrDb).toBeNull();
  });

  it('keeps memory when its name matches, and live signals beat it', () => {
    const heard = [{ ...old(99.9, 'WKIS'), hd: true }];
    const local = localStations([st(99.9, 'WKIS', 4), st(99.1, 'WEDR', 5)], heard, new Map());
    const rows = dialRows(true, local, heard, [{ hz: 99.1e6, snr_db: 19.4, hd: true }], new Map(), 1000);
    expect(rows.map((r) => [r.title, r.type, r.hd, r.snrDb, r.onAir])).toEqual([
      ['WEDR', undefined, true, 19, true],
      ['WKIS', 'Country', true, 20, false],
    ]);
  });

  it('adds a strong signal the list lacks, on real channels only', () => {
    const rows = dialRows(true, new Map(), [], [{ hz: 93.5e6, snr_db: 30 }, { hz: 93.6e6, snr_db: 30 }, { hz: 95.1e6, snr_db: 8 }], new Map(), 0);
    expect(rows.map((r) => r.title)).toEqual(['FM 93.5']);
  });

  it('falls back on memory without a list, leaving out noise', () => {
    const heard = [old(99.9, 'WKIS'), old(99.1, undefined, 25), old(95.1, undefined, 8), old(86.4, undefined, 25)];
    expect(dialRows(false, new Map(), heard, [], new Map(), 1000).map((r) => r.title)).toEqual(['FM 99.1', 'WKIS']);
    expect(localName(false, new Map(), heard, 99.9e6)).toBe(heardName(heard, 99.9e6));
  });

  it('remembers HD, and an edge-of-capture reading does not clear it', () => {
    const a = noteHeard([], [{ hz: 99.9e6, snr_db: 20, hd: true }], new Map(), 1000);
    expect(a[0].hd).toBe(true);
    expect(noteHeard(a, [{ hz: 99.9e6, snr_db: 20, hd: null }], new Map(), 1000 + TOUCH_MS)[0].hd).toBe(true);
  });
});

describe('sortDial', () => {
  const row = (mhz: number, title: string, city: string | undefined, snrDb: number | null) => ({ hz: mhz * 1e6, title, city, snrDb, hd: false, onAir: false });
  const rows = [row(99.9, 'WKIS', 'Boca Raton', 23), row(88.9, 'WDNA', 'Miami', null), row(100.7, 'WHYI', undefined, 30), row(93.1, 'WFEZ', 'Aventura', 8)];
  const order = (key: Parameters<typeof sortDial>[1], desc: boolean) => sortDial(rows, key, desc).map((r) => r.title);

  it('sorts by frequency and call sign, both ways', () => {
    expect(order('freq', false)).toEqual(['WDNA', 'WFEZ', 'WKIS', 'WHYI']);
    expect(order('freq', true)).toEqual(['WHYI', 'WKIS', 'WFEZ', 'WDNA']);
    expect(order('call', false)).toEqual(['WDNA', 'WFEZ', 'WHYI', 'WKIS']);
  });

  it('sorts by what a station plays, the unknown ones last', () => {
    const typed = rows.map((r, i) => ({ ...r, type: ['Top 40', undefined, 'Classic rock', 'Adult hits'][i] }));
    expect(sortDial(typed, 'type', false).map((r) => r.title)).toEqual(['WFEZ', 'WHYI', 'WKIS', 'WDNA']);
  });

  it('keeps rows with nothing in the column last, both ways', () => {
    expect(order('city', false)).toEqual(['WFEZ', 'WKIS', 'WDNA', 'WHYI']);
    expect(order('city', true)).toEqual(['WDNA', 'WKIS', 'WFEZ', 'WHYI']);
    expect(order('signal', true)).toEqual(['WHYI', 'WKIS', 'WFEZ', 'WDNA']);
    expect(order('signal', false)).toEqual(['WFEZ', 'WKIS', 'WHYI', 'WDNA']);
  });

  it('keeps a row in place until its signal has moved HOLD_DB, then moves it once', () => {
    const titles = (rows: ReturnType<typeof row>[], held: Map<number, number | null>) => sortDial(rows, 'signal', true, held).map((r) => r.title);
    let held = holdSignals(new Map(), [row(99.1, 'A', 'x', 20), row(99.9, 'B', 'x', 19)]);
    expect(held.get(99.1e6)).toBe(20);
    // B reads above A, but by less than the hold: order stands
    held = holdSignals(held, [row(99.1, 'A', 'x', 19), row(99.9, 'B', 'x', 21)]);
    expect(titles([row(99.1, 'A', 'x', 19), row(99.9, 'B', 'x', 21)], held)).toEqual(['A', 'B']);
    // B past the hold: it moves, and A's held reading is still its first
    const late = [row(99.1, 'A', 'x', 19), row(99.9, 'B', 'x', 19 + HOLD_DB)];
    held = holdSignals(held, late);
    expect(held.get(99.9e6)).toBe(19 + HOLD_DB);
    expect(held.get(99.1e6)).toBe(20);
    expect(titles(late, held)).toEqual(['B', 'A']);
  });

  it('starts a held reading at the live one, and follows a reading that goes missing', () => {
    let held = holdSignals(new Map(), [row(99.1, 'A', 'x', null)]);
    expect(held.get(99.1e6)).toBeNull();
    held = holdSignals(held, [row(99.1, 'A', 'x', 7)]);
    expect(held.get(99.1e6)).toBe(7);
    held = holdSignals(held, [row(99.1, 'A', 'x', null)]);
    expect(held.get(99.1e6)).toBeNull();
    // a station gone from the rows is forgotten
    expect(holdSignals(held, []).size).toBe(0);
  });

  it('leaves its input alone', () => {
    sortDial(rows, 'call', false);
    expect(rows[0].title).toBe('WKIS');
  });
});
