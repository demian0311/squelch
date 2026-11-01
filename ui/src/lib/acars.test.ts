import { describe, expect, it } from 'vitest';
import { acarsSummary, acarsWho, mergeMessages } from './acars';
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
