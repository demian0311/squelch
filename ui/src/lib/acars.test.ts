import { describe, expect, it } from 'vitest';
import { acarsPosition, acarsSummary, acarsWho, filterMessages, joinBlocks, mergeMessages } from './acars';
import type { AcarsMessage } from './api';

const m = (id: number, at_ms: number, more: Partial<AcarsMessage> = {}): AcarsMessage => ({
  id,
  at_ms,
  hz: 131_550_000,
  mode: '2',
  registration: 'N8722Q',
  ack: null,
  label: 'H1',
  block_id: '4',
  more: false,
  msn: 'M51A',
  flight: 'UA0451',
  text: 'POS',
  ...more,
});

describe('mergeMessages', () => {
  it('puts new messages first, newest first', () => {
    const list = [m(2, 2000), m(1, 1000)];
    expect(mergeMessages(list, [m(2, 2000), m(3, 3000), m(4, 4000)]).map((x) => x.id)).toEqual([4, 3, 2, 1]);
  });
  it('returns the same list when nothing is new', () => {
    const list = [m(1, 1000)];
    expect(mergeMessages(list, [m(1, 1000)])).toBe(list);
  });
  it('keeps messages from a restarted decoder whose ids start over', () => {
    expect(mergeMessages([m(1, 1000)], [m(1, 9000)]).map((x) => x.at_ms)).toEqual([9000, 1000]);
  });
  it('drops the oldest past the limit', () => {
    expect(mergeMessages([m(2, 2000), m(1, 1000)], [m(3, 3000)], 2).map((x) => x.id)).toEqual([3, 2]);
  });
});

describe('acarsSummary', () => {
  it('counts channels and messages', () => {
    expect(acarsSummary({ channels: [1, 2], messages: [], heard: 0 }, 0)).toBe('Listening on 2 channels · none yet');
    expect(acarsSummary({ channels: [1], messages: [], heard: 1 }, 1)).toBe('Listening on 1 channel · 1 message');
    expect(acarsSummary({ channels: [], messages: [], heard: 0 }, 0)).toMatch(/^No ACARS channel/);
  });
});

describe('acarsWho', () => {
  it('prefers the flight, then the registration', () => {
    expect(acarsWho(m(1, 0))).toBe('UA0451');
    expect(acarsWho(m(1, 0, { flight: '' }))).toBe('N8722Q');
    expect(acarsWho(m(1, 0, { flight: '', registration: '' }))).toBe('Ground station');
  });
});

describe('acarsPosition', () => {
  const at = (label: string, text: string) => acarsPosition({ label, text });
  it('reads POS with tenths of minutes', () => {
    const p = at('20', 'POSN38160W077075,,211733,360,OTT,212041,,N42,19689,40,544')!;
    expect(p.lat).toBeCloseTo(38.2667, 3);
    expect(p.lon).toBeCloseTo(-77.125, 3);
    expect(at('H1', 'POSN43312W123174,EASON,215754,370,EBINY,220601,ELENN,M48,02216,185/TS215754,0921227A40')!.lat).toBeCloseTo(43.52, 2);
  });
  it('reads decimal degrees', () => {
    const p = at('21', 'POSN 34.428W 77.692, 230,065030,31995,32372,  45,-52,094730,TJSJ')!;
    expect(p).toEqual({ lat: 34.428, lon: -77.692 });
    expect(at('16', 'N 29.763/W 95.530')).toEqual({ lat: 29.763, lon: -95.53 });
    expect(at('16', 'PC0397/04AUG22 12:08:30,12239,1527, 244,N 59.822 E 29.693')).toEqual({ lat: 59.822, lon: 29.693 });
  });
  it('reads degrees, minutes, seconds', () => {
    const p = at('22', 'N 404630W 735400,-------,042530,9151, ,      , ,M  5,26173  15, 206,')!;
    expect(p.lat).toBeCloseTo(40.775, 3);
    expect(p.lon).toBeCloseTo(-73.9, 3);
  });
  it('leaves other labels and text alone', () => {
    expect(at('H1', 'REQPWI/WQ350')).toBeNull();
    expect(at('5Z', 'POSN38160W077075')).toBeNull();
    expect(at('20', 'POSN99160W077075')).toBeNull();
  });
});

describe('joinBlocks', () => {
  const msg = (at_ms: number, text: string, more = false, msn = 'M12A', registration = 'N1'): AcarsMessage => ({ id: at_ms, at_ms, hz: 131.55e6, mode: '2', registration, ack: null, label: 'H1', block_id: '1', more, msn, flight: 'UA1', text });
  it('joins blocks of one message, newest first', () => {
    const out = joinBlocks([msg(3000, 'c', false, 'M12C'), msg(2000, 'b', true, 'M12B'), msg(1000, 'a', true)]);
    expect(out.map((m) => m.text)).toEqual(['abc']);
    expect(out[0].more).toBe(false);
  });
  it('counts a retransmission once', () => {
    expect(joinBlocks([msg(2000, 'hello'), msg(1000, 'hello')]).length).toBe(1);
  });
  it('drops the header each block repeats', () => {
    const out = joinBlocks([msg(2000, '#DFB,0270/', false, 'D32B'), msg(1000, '#DFBA04/,54', true, 'D32A')]);
    expect(out.map((m) => m.text)).toEqual(['#DFBA04/,54,0270/']);
  });
  it('counts a retransmitted later block once', () => {
    const out = joinBlocks([msg(3000, '#DFBbb', false, 'D32B'), msg(2000, '#DFBbb', false, 'D32B'), msg(1000, '#DFBaa', true, 'D32A')]);
    expect(out.map((m) => m.text)).toEqual(['#DFBaabb']);
  });
  it('marks blocks not heard, on air N390FR sent B, E, F, H', () => {
    const out = joinBlocks([msg(4000, '#DFBh', true, 'D92H'), msg(3000, '#DFBf', true, 'D92F'), msg(2000, '#DFBe', true, 'D92E'), msg(1000, '#DFBb', true, 'D92B')]);
    expect(out.map((m) => m.text)).toEqual(['… #DFBb … ef … h …']);
  });
  it('sorts by the last block heard', () => {
    const out = joinBlocks([msg(3000, 'b', false, 'M12B'), msg(2000, 'other', false, 'M13A', 'N2'), msg(1000, 'a', true)]);
    expect(out.map((m) => m.text)).toEqual(['ab', 'other']);
  });
  it('leaves an empty block empty', () => {
    expect(joinBlocks([msg(1000, '', false, 'M12C')])[0].text).toBe('');
  });
  it('still finds a position in a report that joined nothing', () => {
    const [m] = joinBlocks([msg(1000, 'POSN 26.082W 80.075, 229', false, 'M63A')]);
    expect(acarsPosition({ label: '21', text: m.text })).toEqual({ lat: 26.082, lon: -80.075 });
  });
  it('keeps other aircraft and later messages apart', () => {
    expect(joinBlocks([msg(2000, 'b', false, 'M12B', 'N2'), msg(1000, 'a', true)]).length).toBe(2);
    expect(joinBlocks([msg(500_000, 'b'), msg(1000, 'a', true)]).length).toBe(2);
  });
});

describe('filterMessages', () => {
  it('needs every word somewhere in the message', () => {
    const m = { id: 1, at_ms: 1, hz: 1, mode: '2', registration: 'N1', ack: null, label: 'Q0', block_id: '1', more: false, msn: '', flight: 'UA1', text: 'gate B4' } as AcarsMessage;
    expect(filterMessages([m], 'ua1 gate').length).toBe(1);
    expect(filterMessages([m], 'link test').length).toBe(1);
    expect(filterMessages([m], 'ua1 cargo').length).toBe(0);
  });
});
