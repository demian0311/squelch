import { afterEach, describe, expect, it } from 'vitest';
import type { Band, BandPlan } from './api';
import { binAt, allBookmarks, bandAt, captureCenter, bookmarksByBand, namedFrequencies, notOnAir, bandEntryHz, outOfBand, fmtHz, label, modeAt, anchor, Persistence, placeCards, rdsNow, snapTune, sondeDrift, sondeSummary, withRds, withSondes, withoutDeadAir } from './radio';
import type { Rds, Sonde } from './api';
import { channelAt, channelList, channelTicks, quietChannels, setRepeaters } from './channels';

const plan: BandPlan = {
  groups: [{ id: 'ham', name: 'Ham' }],
  bands: [
    { id: '2m', group: 'ham', name: '2 m', icon: 'tower', mode: 'nfm', low_mhz: 144, high_mhz: 148, start_mhz: 146.52, bookmarks: [{ mhz: 146.52, name: 'Simplex calling' }, { mhz: 144.39, name: 'APRS', mode: 'none' }] },
    { id: 'iss', group: 'ham', name: 'Space station', icon: 'star', mode: 'nfm', low_mhz: 145.6, high_mhz: 146, start_mhz: 145.8, bookmarks: [] },
    { id: 'fm', group: 'ham', name: 'FM radio', icon: 'radio', mode: 'wfm', low_mhz: 87.8, high_mhz: 108, start_mhz: 101.1, bookmarks: [] },
  ],
};
const saved = allBookmarks(plan);

describe('notOnAir', () => {
  const marks = allBookmarks({ ...plan, bands: [{ ...plan.bands[0], bookmarks: [{ mhz: 146.52, name: 'Calling' }, { mhz: 144.39, name: 'APRS' }, { mhz: 146.94, name: 'Club' }] }] });
  it('keeps those in view that nothing is on', () => {
    expect(notOnAir(marks, 144e6, 148e6, []).map((s) => s.name)).toEqual(['APRS', 'Calling', 'Club']);
  });
  it('drops one with a signal on it, and one out of view', () => {
    expect(notOnAir(marks, 146e6, 148e6, [{ hz: 146.5215e6, widthHz: 0 }]).map((s) => s.name)).toEqual(['Club']);
    expect(notOnAir(marks, 146e6, 146.6e6, []).map((s) => s.name)).toEqual(['Calling']);
  });
});

describe('bookmarksByBand', () => {
  it('leaves out bands and groups without bookmarks, and keeps the rail order', () => {
    const got = bookmarksByBand(plan, saved);
    expect(got.map((g) => g.group.id)).toEqual(['ham']);
    expect(got[0].bands.map((b) => b.band.id)).toEqual(['2m']);
    expect(got[0].bands[0].marks.map((s) => s.mhz)).toEqual([144.39, 146.52]);
  });
});

describe('namedFrequencies', () => {
  it('lists built-in names and bookmarks, and a bookmark replaces the name it sits on', () => {
    const named: BandPlan = {
      ...plan,
      bands: [{ ...plan.bands[0], names: [{ mhz: 146.52, name: 'Simplex calling' }, { mhz: 144.39, name: 'APRS' }], bookmarks: [{ mhz: 146.52, name: 'My calling' }, { mhz: 145.5, name: 'Net' }] }],
    };
    expect(namedFrequencies(named, allBookmarks(named)).map((s) => s.name)).toEqual(['APRS', 'Net', 'My calling']);
  });
  it('does not make a name a bookmark', () => {
    const named: BandPlan = { ...plan, bands: [{ ...plan.bands[0], names: [{ mhz: 144.39, name: 'APRS' }], bookmarks: [] }] };
    expect(allBookmarks(named)).toEqual([]);
  });
});

describe('captureCenter', () => {
  const half = 1_080_000;
  const dc = 60_000;
  it('stays put while the view and the tuned frequency fit', () => {
    expect(captureCenter(101.4e6, 101e6, 101.5e6, 101.1e6, half, dc)).toBe(101.4e6);
  });
  it('moves just enough room to keep the tuned frequency when the view leaves', () => {
    const c = captureCenter(101.4e6, 102e6, 102.8e6, 101.1e6, half, dc);
    expect(c).toBe(101.95e6);
    expect(Math.abs(101.1e6 - c)).toBeLessThanOrEqual(half);
    expect(102.8e6 - c).toBeLessThanOrEqual(half);
  });
  it('follows the view once the tuned frequency cannot fit, and stops moving when the view fits', () => {
    const c = captureCenter(101.4e6, 104e6, 105e6, 101.1e6, half, dc);
    expect(c).toBe(104.5e6);
    expect(captureCenter(c, 104.1e6, 104.9e6, 101.1e6, half, dc)).toBe(c);
  });
  it('keeps the tuned frequency off the DC spike', () => {
    expect(captureCenter(90e6, 101e6, 101.2e6, 101.1e6, half, dc)).toBe(101.16e6);
  });
});

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
  it('returns to the last frequency used in a band', () => {
    const fm = plan.bands.find((b) => b.id === 'fm')!;
    expect(bandEntryHz(fm, {})).toBe(101_100_000);
    expect(bandEntryHz(fm, { fm: 94_700_000 })).toBe(94_700_000);
    // a remembered frequency the band no longer covers (bands file edited) falls back to the start
    expect(bandEntryHz(fm, { fm: 150_000_000 })).toBe(101_100_000);
  });
  it('lets a bookmark override the band mode', () => {
    expect(modeAt(plan, saved, 144.39e6, 'am')).toBe('none');
    expect(modeAt(plan, saved, 146.6e6, 'am')).toBe('nfm');
    expect(modeAt(plan, saved, 50e6, 'am')).toBe('am');
    const cb = { ...plan, bands: [...plan.bands, { ...plan.bands[0], id: 'cb', low_mhz: 26.95, high_mhz: 27.42, mode: 'am' as const }] };
    expect(modeAt(cb, [], 27_385_000, 'nfm')).toBe('lsb');
    expect(modeAt(cb, [], 27_185_000, 'nfm')).toBe('am');
  });
});

describe('out of band', () => {
  it('shades the part of the view past each edge', () => {
    expect(outOfBand(147e6, 149e6, 144e6, 148e6)).toEqual({ below: 0, above: 50 });
    expect(outOfBand(143e6, 145e6, 144e6, 148e6)).toEqual({ below: 50, above: 0 });
  });
  it('shades nothing inside the band and everything past it', () => {
    expect(outOfBand(145e6, 146e6, 144e6, 148e6)).toEqual({ below: 0, above: 0 });
    expect(outOfBand(150e6, 151e6, 144e6, 148e6)).toEqual({ below: 0, above: 100 });
  });
});

describe('labels', () => {
  it('names a signal from a bookmark', () => {
    const s = label(plan, saved, { hz: 146_521_200, width_hz: 9_000, snr_db: 20 });
    expect(s.title).toBe('Simplex calling');
    expect(s.saved).toBe(true);
  });
  it('names broadcast FM by its channel', () => {
    const s = label(plan, saved, { hz: 101_097_000, width_hz: 170_000, snr_db: 25 });
    expect(s.title).toBe('FM 101.1');
    expect(s.byFreq).toBe(true);
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
    const wide = placeCards([sig(100, 5), sig(110, 20)], (a, b) => (a.hz + b.hz > 1000 ? 5 : 50));
    expect(wide.length).toBe(1);
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
    expect(named.byFreq).toBe(false);
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

const cbPlan: BandPlan = { groups: [], bands: [{ id: 'cb', group: 'x', name: 'CB radio', icon: 'truck', mode: 'am', low_mhz: 26.95, high_mhz: 27.42, start_mhz: 27.185, bookmarks: [] }] };

describe('channel plans', () => {
  it('names a signal after its channel, out-of-order ones included', () => {
    expect(label(cbPlan, [], { hz: 27_186_200, width_hz: 6_000, snr_db: 15 }).title).toBe('CB 19');
    expect(label(cbPlan, [], { hz: 27_255_000, width_hz: 6_000, snr_db: 15 }).title).toBe('CB 23');
    expect(label(cbPlan, [], { hz: 27_255_000, width_hz: 6_000, snr_db: 15 }).channelHz).toBe(27_255_000);
  });
  it('tunes the channel when a click lands near one', () => {
    expect(snapTune(27_187_000, [], [], 1000, 'am', 'cb')).toBe(27_185_000);
    expect(snapTune(27_189_500, [{ hz: 27_186_200, width_hz: 6_000, snr_db: 15 }], [], 500, 'am', 'cb')).toBe(27_185_000);
  });
});

describe('repeaters', () => {
  const k0ml = { id: 'k0ml', call: 'K0ML', city: 'Northglenn', mhz: 147.045, lat: 39.886, lon: -104.987, shift: '+', tone: '123.0', modes: [] };
  const digi = { id: 'digi', call: 'W0DMR', city: 'Boulder', mhz: 447.0, lat: 40.015, lon: -105.27, shift: '-', tone: 'D023', modes: ['DMR'] };
  const ham: BandPlan = {
    groups: [],
    bands: [...plan.bands, { id: '70cm', group: 'ham', name: '70 cm', icon: 'tower', mode: 'nfm', low_mhz: 420, high_mhz: 450, start_mhz: 446, bookmarks: [] }],
  };
  afterEach(() => setRepeaters([], []));
  it('names a signal on a repeater output, and says where it is and how to reach it', () => {
    setRepeaters([k0ml, digi], ham.bands);
    expect(label(ham, saved, { hz: 147_046_000, width_hz: 12_000, snr_db: 20 }).title).toBe('K0ML');
    expect(channelAt('2m', 147_045_000)?.group).toBe('Northglenn · +600 kHz · 123.0 Hz');
    expect(channelAt('70cm', 447_000_000)?.group).toBe('Boulder · −5 MHz · DCS 023 · DMR');
  });
  it("doesn't let a lone repeater claim the whole band", () => {
    setRepeaters([k0ml], ham.bands);
    expect(label(ham, saved, { hz: 146_940_000, width_hz: 12_000, snr_db: 20 }).title).toBe('2 m signal');
  });
  it('tunes a 6 m repeater in FM though the band is sideband', () => {
    const six: BandPlan = { groups: [], bands: [{ id: '6m', group: 'ham', name: '6 m', icon: 'wave', mode: 'usb', low_mhz: 50, high_mhz: 54, start_mhz: 50.2, bookmarks: [] }] };
    setRepeaters([{ ...k0ml, mhz: 53.09, shift: '-' }], six.bands);
    expect(modeAt(six, [], 53_090_000, 'usb')).toBe('nfm');
    expect(modeAt(six, [], 50_125_000, 'usb')).toBe('usb');
    expect(channelAt('6m', 53_090_000)?.group).toBe('Northglenn · − offset · 123.0 Hz');
  });
  it('leaves fixed channel plans alone', () => {
    setRepeaters([{ ...k0ml, mhz: 27.185 }], cbPlan.bands);
    expect(channelAt('cb', 27_185_000)?.name).toBe('CB 19');
  });
});

describe('channel list', () => {
  it('gives FRS and GMRS one plan, by what each channel is for, with the repeater inputs', () => {
    const list = channelList('walkie', [{ hz: 467_550_400, snr_db: 12 }, { hz: 462_675_200, snr_db: 30 }, { hz: 462_675_000, snr_db: 20 }]);
    expect(list.map((s) => s.group)).toEqual(['Everyday', 'Short range', 'Repeaters', 'Repeater inputs']);
    const all = list.flatMap((s) => s.channels);
    expect(all[14].channel).toMatchObject({ hz: 462_550_000, name: 'Channel 15' });
    expect(all.filter((c) => c.live).map((c) => [c.channel.name, c.snr])).toEqual([['Channel 20', 30], ['Channel 15R', 12]]);
    expect(channelList('frs_only', [])).toEqual(channelList('gmrs', []));
  });
  it('puts a plan without groups in one section', () => {
    const sections = channelList('cb', []);
    expect(sections.flatMap((s) => s.channels).length).toBe(40);
    expect(sections.flatMap((s) => s.channels).find((c) => c.channel.name === 'CB 9')?.channel.group).toBe('Emergency');
  });
  it('has the five MURS channels', () => {
    expect(channelList('murs', []).flatMap((s) => s.channels).map((c) => c.channel.hz)).toEqual([151_820_000, 151_880_000, 151_940_000, 154_570_000, 154_600_000]);
  });
  it('lists nothing for bands without a fixed plan', () => {
    expect(channelList('2m', [])).toEqual([]);
    expect(channelList(undefined, [])).toEqual([]);
  });
});

describe('quiet channels', () => {
  it('lists every walkie-talkie channel and repeater input without a signal', () => {
    const all = quietChannels('walkie', 462e6, 468e6, []);
    expect(all.length).toBe(30);
    expect(all.find((c) => c.hz === 467_550_000)).toMatchObject({ name: 'Channel 15R', group: 'Repeater inputs' });
    const left = quietChannels('walkie', 462e6, 468e6, [{ hz: 467_551_000 }, { hz: 462_562_000 }]);
    expect(left.map((c) => c.name)).not.toContain('Channel 15R');
    expect(left.map((c) => c.name)).not.toContain('Channel 1');
    expect(left.length).toBe(28);
    expect(quietChannels('fm', 88e6, 108e6, [])).toEqual([]);
  });
});

describe('channel scale', () => {
  it('marks every channel and thins the numbers to fit', () => {
    // all 40 CB channels across 440 px: 10 kHz is 10 px, so every third number shows
    const t = channelTicks('cb', 26_960_000, 27_400_000 + 10_000, 450);
    expect(t.length).toBe(40);
    const shown = t.filter((x) => x.text).map((x) => x.text);
    expect(shown.length).toBeLessThan(16);
    expect(channelTicks('marine', 157.15e6, 157.45e6, 800).map((t) => t.text).filter(Boolean)).toContain('24');
    expect(channelTicks('fm', 88e6, 108e6, 800)).toEqual([]);
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

describe('dead air', () => {
  const b = (id: string, group: string) => ({ id, group, name: id, icon: 'radio', mode: 'nfm' as const, low_mhz: 1, high_mhz: 2, start_mhz: 1, bookmarks: [] });
  const p: BandPlan = {
    groups: [{ id: 'listen', name: 'Listen' }, { id: 'ham', name: 'Ham' }, { id: 'data', name: 'Data and tracking' }],
    bands: [b('fm', 'listen'), b('murs', 'listen'), b('rail', 'listen'), b('mil', 'listen'), b('10m', 'ham'), b('6m', 'ham'), b('2m', 'ham'), b('125', 'ham'), b('70cm', 'ham'), b('33cm', 'ham'), b('23cm', 'ham'), b('adsb', 'data'), b('sonde', 'data')],
  } as BandPlan;

  it('drops the data group, MURS, railroad, military aircraft and ham except 2 m and 70 cm', () => {
    const out = withoutDeadAir(p);
    expect(out.bands.map((x) => x.id)).toEqual(['fm', '2m', '70cm', 'adsb']);
    expect(out.groups.map((g) => g.id)).toEqual(['listen', 'ham', 'data']);
  });
  it('enters the ACARS band where the most channels fit, whatever was last tuned', () => {
    const b = { id: 'acars', group: 'data', name: 'x', icon: 'msg', mode: 'am', low_mhz: 129, high_mhz: 131.7, start_mhz: 130.4, decoder: 'acars', bookmarks: [] };
    expect(bandEntryHz(b as Band, { acars: 129_600_500 })).toBe(130_500_000);
  });
  it('keeps the aircraft decoders in an otherwise quiet group', () => {
    const band = (id: string) => ({ id, group: 'data', name: id, icon: 'pin', mode: 'none', low_mhz: 1, high_mhz: 2, start_mhz: 1, bookmarks: [] });
    const out = withoutDeadAir({ groups: [{ id: 'data', name: 'Data' }], bands: [band('adsb'), band('acars'), band('sonde')] } as BandPlan);
    expect(out.bands.map((x) => x.id)).toEqual(['adsb', 'acars']);
    expect(out.groups.map((g) => g.id)).toEqual(['data']);
  });
});

describe('binAt', () => {
  it('puts a frequency in the bin centred on it', () => {
    expect(binAt(1000, 1000, 100)).toBe(0);
    expect(binAt(1049, 1000, 100)).toBe(0);
    expect(binAt(1051, 1000, 100)).toBe(1);
    expect(binAt(1500, 1000, 100)).toBe(5);
  });
});
