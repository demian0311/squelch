import { describe, expect, it } from 'vitest';
import type { BandPlan } from './api';
import { allBookmarks, bandAt, fmtHz, label, modeAt, anchor, Persistence, placeCards, rdsNow, snapTune, sondeDrift, sondeSummary, withRds, withSondes } from './radio';
import type { Rds, Sonde } from './api';

const plan: BandPlan = {
  groups: [{ id: 'ham', name: 'Ham' }],
  bands: [
    { id: '2m', group: 'ham', name: '2 m', icon: 'tower', mode: 'nfm', low_mhz: 144, high_mhz: 148, start_mhz: 146.52, bookmarks: [{ mhz: 146.52, name: 'Simplex calling' }, { mhz: 144.39, name: 'APRS', mode: 'none' }] },
    { id: 'iss', group: 'ham', name: 'Space station', icon: 'star', mode: 'nfm', low_mhz: 145.6, high_mhz: 146, start_mhz: 145.8, bookmarks: [] },
    { id: 'fm', group: 'ham', name: 'FM radio', icon: 'radio', mode: 'wfm', low_mhz: 87.8, high_mhz: 108, start_mhz: 101.1, bookmarks: [] },
  ],
};
const saved = allBookmarks(plan);

describe('frequency text', () => {
  it('groups digits like a radio dial', () => {
    expect(fmtHz(146_520_000)).toBe('146.520.000');
    expect(fmtHz(88_100_000)).toBe('88.100.000');
    expect(fmtHz(1_090_000_000)).toBe('1090.000.000');
  });
});

describe('band lookup', () => {
  it('prefers the narrowest band', () => {
    expect(bandAt(plan, 145.8e6)?.id).toBe('iss');
    expect(bandAt(plan, 146.52e6)?.id).toBe('2m');
    expect(bandAt(plan, 50e6)).toBeUndefined();
  });
  it('lets a bookmark override the band mode', () => {
    expect(modeAt(plan, saved, 144.39e6, 'am')).toBe('none');
    expect(modeAt(plan, saved, 146.6e6, 'am')).toBe('nfm');
    expect(modeAt(plan, saved, 50e6, 'am')).toBe('am');
  });
});

describe('labels', () => {
  it('names a signal from a bookmark', () => {
    const s = label(plan, saved, { hz: 146_521_200, width_hz: 9_000, snr_db: 20 });
    expect(s.title).toBe('Simplex calling');
    expect(s.saved).toBe(true);
  });
  it('names broadcast FM by its channel', () => {
    expect(label(plan, saved, { hz: 101_097_000, width_hz: 170_000, snr_db: 25 }).title).toBe('FM 101.1');
  });
});

describe('card placement', () => {
  const sig = (hz: number, snrDb: number) => ({ hz, snrDb });
  it('keeps the strongest of overlapping signals', () => {
    const p = placeCards([sig(100, 5), sig(110, 20), sig(500, 3)], 50);
    expect(p.map((c) => c.item.hz)).toEqual([110, 500]);
    expect(p[0].hidden.map((h) => h.hz)).toEqual([100]);
  });
  it('always shows the one being listened to', () => {
    const p = placeCards([sig(100, 5), sig(110, 20)], 50, (s) => s.hz === 100);
    expect(p.map((c) => c.item.hz)).toEqual([100]);
  });
});

describe('steady carriers', () => {
  const sig = { hz: 146_288_000, widthHz: 2_300, snrDb: 12, saved: false };
  it('flags a narrow signal that never switches off', () => {
    const p = new Persistence(5_000, 15_000, 20_000);
    for (let t = 0; t <= 30_000; t += 200) p.update([sig.hz], t);
    expect(p.steady(sig, 30_000, 75)).toBe(true);
  });
  it('leaves a signal that comes and goes', () => {
    const p = new Persistence(5_000, 15_000, 20_000);
    for (let t = 0; t <= 30_000; t += 200) p.update(t % 4_000 < 2_000 ? [sig.hz] : [], t);
    expect(p.steady(sig, 30_000, 75)).toBe(false);
  });
  it('never hides saved, wide or strong signals', () => {
    const p = new Persistence(5_000, 15_000, 20_000);
    for (let t = 0; t <= 30_000; t += 200) p.update([sig.hz], t);
    expect(p.steady({ ...sig, saved: true }, 30_000, 75)).toBe(false);
    expect(p.steady({ ...sig, widthHz: 180_000 }, 30_000, 75)).toBe(false);
    expect(p.steady({ ...sig, snrDb: 30 }, 30_000, 75)).toBe(false);
  });
});

describe('station data', () => {
  const kbpi: Rds = { pi: 5170, call: 'KBPI', ps: 'KBPI -', radiotext: 'KBPI - Shoot To Thrill - AC/DC', pty: 'Rock', title: 'Shoot To Thrill', artist: 'AC/DC' };
  it('prefers tagged artist and title, else the text', () => {
    expect(rdsNow(kbpi)).toBe('AC/DC – Shoot To Thrill');
    expect(rdsNow({ ...kbpi, title: 'Shoot To Thri', radiotext: 'Something else' })).toBe('Something else');
    expect(rdsNow({ ...kbpi, title: null, radiotext: null })).toBe('Rock');
  });
  it('names FM cards from remembered stations', () => {
    const sig = label(plan, saved, { hz: 107_903_000, width_hz: 150_000, snr_db: 25 });
    const [named] = withRds([sig], new Map([[107_900_000, kbpi]]));
    expect(named.title).toBe('KBPI 107.9');
    expect(named.sub).toBe('AC/DC – Shoot To Thrill');
  });
});

describe('weather balloons', () => {
  const sonde: Sonde = { kind: 'DFM-17', serial: '23038743', hz: 403_301_000, frame: 7, time: '19:45:34', lat: 39.7, lon: -105, alt_m: 12_340, speed_ms: 18.4, heading_deg: 85, climb_ms: 5.12, frames: 30, age_s: 1 };
  it('describes height, climb and drift', () => {
    expect(sondeSummary(sonde)).toBe('12.3 km · rising 5.1 m/s');
    expect(sondeSummary({ ...sonde, climb_ms: -14 })).toBe('12.3 km · falling 14.0 m/s');
    expect(sondeSummary({ ...sonde, alt_m: null })).toBe('Heard, no GPS position yet');
    expect(sondeDrift(sonde)).toBe('drifting east at 18 m/s');
  });
  it('names the signal it was decoded on', () => {
    const sig = label(plan, saved, { hz: 403_300_000, width_hz: 12_000, snr_db: 15 });
    const [named] = withSondes([sig], [sonde]);
    expect(named.title).toBe('DFM-17 23038743');
    expect(named.saved).toBe(true);
  });
});

describe('card anchors', () => {
  it('hold still through small wobble and follow a real move', () => {
    expect(anchor(99_498_300, [99_497_000], 5_000)).toBe(99_497_000);
    expect(anchor(99_520_000, [99_497_000], 5_000)).toBe(99_520_000);
    expect(anchor(146_520_000, [], 5_000)).toBe(146_520_000);
  });
});

describe('click to tune', () => {
  it('snaps only when the click is within a few pixels', () => {
    // 1 kHz per pixel: a bookmark 3 px away snaps, a signal 3 px away snaps
    expect(snapTune(146_523_000, [], saved, 1000, 'nfm')).toBe(146_520_000);
    expect(snapTune(147_001_000, [{ hz: 146_998_000, width_hz: 10_000, snr_db: 20 }], saved, 1000, 'nfm')).toBe(146_998_000);
  });
  it('tunes where the click lands when nothing is close', () => {
    expect(snapTune(147_031_200, [{ hz: 146_998_000, width_hz: 60_000, snr_db: 20 }], saved, 1000, 'nfm')).toBe(147_031_000);
    expect(snapTune(101_133_000, [], saved, 1000, 'wfm')).toBe(101_130_000);
  });
});
