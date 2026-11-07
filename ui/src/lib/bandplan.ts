// What each part of a ham band is for, drawn as the ruler above the waterfall (BandPlanRuler.svelte).
// Source: the ARRL band plan, https://www.arrl.org/band-plan, read 2026-11-06. Simplified where
// the plan nests one use inside another (6 m digital repeaters, 2 m packet channels): the wider use wins.
// A segment's `mode` is what tuning inside it switches to (`modeAt`); none leaves the band's own.

import type { Mode } from './api';

/** One colour per kind of use; `rin` is drawn hatched, since you hear a repeater on its output. */
export type Use = 'weak' | 'data' | 'beacon' | 'sat' | 'rout' | 'rin' | 'rpt' | 'simplex' | 'other';

export interface Segment {
  lo: number;
  hi: number;
  use: Use;
  /** Short enough to sit inside the block. */
  name: string;
  mode?: Mode;
}

/** What each colour means, for the tooltip and the guide. */
export const USE_WORDS: Record<Use, string> = {
  weak: 'Weak signals: CW and SSB',
  data: 'Data: RTTY, packet',
  beacon: 'Beacons',
  sat: 'Satellites',
  rout: 'Repeater outputs: what you hear',
  rin: 'Repeater inputs: radios talking to a repeater',
  rpt: 'Repeaters, inputs and outputs by local choice',
  simplex: 'FM simplex: radio to radio',
  other: 'Links, TV, remote control, experiments',
};

const mhz = (m: number) => Math.round(m * 1e6);
type Row = [number, number, Use, string, Mode?];
const seg = (rows: Row[]): Segment[] => rows.map(([lo, hi, use, name, mode]) => ({ lo: mhz(lo), hi: mhz(hi), use, name, mode }));

export const BAND_PLANS: Record<string, Segment[]> = {
  '10m': seg([
    [28.0, 28.07, 'weak', 'CW', 'usb'],
    [28.07, 28.15, 'data', 'RTTY', 'usb'],
    [28.15, 28.19, 'weak', 'CW', 'usb'],
    [28.2, 28.3, 'beacon', 'Beacons', 'usb'],
    [28.3, 29.0, 'weak', 'SSB voice', 'usb'],
    [29.0, 29.2, 'weak', 'AM voice', 'am'],
    [29.2, 29.3, 'weak', 'SSB voice', 'usb'],
    [29.3, 29.51, 'sat', 'Satellites', 'usb'],
    [29.52, 29.59, 'rin', 'Rpt in', 'nfm'],
    [29.59, 29.61, 'simplex', 'Simplex', 'nfm'],
    [29.61, 29.7, 'rout', 'Rpt out', 'nfm'],
  ]),
  '6m': seg([
    [50.0, 50.1, 'weak', 'CW, beacons', 'usb'],
    [50.1, 50.3, 'weak', 'SSB · 50.125', 'usb'],
    [50.3, 50.6, 'weak', 'All modes', 'usb'],
    [50.6, 50.8, 'data', 'Data', 'usb'],
    [50.8, 51.0, 'other', 'Remote control'],
    [51.0, 51.1, 'weak', 'Pacific DX', 'usb'],
    [51.12, 51.48, 'rin', 'Rpt in', 'nfm'],
    [51.5, 51.6, 'simplex', 'Simplex', 'nfm'],
    [51.62, 51.98, 'rout', 'Rpt out', 'nfm'],
    [52.0, 52.48, 'rin', 'Rpt in', 'nfm'],
    [52.5, 52.98, 'rout', 'Rpt out · 52.525', 'nfm'],
    [53.0, 53.48, 'rin', 'Rpt in', 'nfm'],
    [53.5, 53.98, 'rout', 'Rpt out', 'nfm'],
  ]),
  '2m': seg([
    [144.0, 144.1, 'weak', 'CW', 'usb'],
    [144.1, 144.275, 'weak', 'SSB · 144.200', 'usb'],
    [144.275, 144.3, 'beacon', 'Beacons', 'usb'],
    [144.3, 144.5, 'sat', 'Satellites', 'nfm'],
    [144.5, 144.6, 'other', 'Translators'],
    [144.6, 144.9, 'rin', 'Rpt in', 'nfm'],
    [144.9, 145.1, 'simplex', 'Simplex, packet', 'nfm'],
    [145.1, 145.2, 'other', 'Translators'],
    [145.2, 145.5, 'rout', 'Rpt out', 'nfm'],
    [145.5, 145.8, 'other', 'Experimental'],
    [145.8, 146.0, 'sat', 'Satellites only', 'nfm'],
    [146.0, 146.4, 'rin', 'Rpt in', 'nfm'],
    [146.4, 146.6, 'simplex', 'Simplex · 146.52', 'nfm'],
    [146.6, 147.4, 'rout', 'Rpt out', 'nfm'],
    [147.4, 147.6, 'simplex', 'Simplex', 'nfm'],
    [147.6, 148.0, 'rin', 'Rpt in', 'nfm'],
  ]),
  '70cm': seg([
    [420.0, 426.0, 'other', 'TV, links'],
    [426.0, 432.0, 'other', 'TV simplex'],
    [432.0, 432.1, 'weak', 'CW', 'usb'],
    [432.1, 432.3, 'weak', 'Weak signal · 432.100', 'usb'],
    [432.3, 432.4, 'beacon', 'Beacons', 'usb'],
    [432.4, 433.0, 'weak', 'Weak signal', 'usb'],
    [433.0, 435.0, 'other', 'Links'],
    [435.0, 438.0, 'sat', 'Satellites only', 'nfm'],
    [438.0, 442.0, 'other', 'TV in, links'],
    [442.0, 445.0, 'rpt', 'Repeaters', 'nfm'],
    [445.0, 447.0, 'simplex', 'Simplex, links · 446.0', 'nfm'],
    [447.0, 450.0, 'rpt', 'Repeaters', 'nfm'],
  ]),
};

/** The segment covering `hz` on band `bandId`, if that band has a plan. */
export function segmentAt(bandId: string | undefined, hz: number): Segment | undefined {
  if (!bandId) return undefined;
  return BAND_PLANS[bandId]?.find((s) => hz >= s.lo && hz < s.hi);
}

/** The segments inside the view, clipped to it, as percentages of its width. */
export function segmentsIn(bandId: string | undefined, vs: number, ve: number): (Segment & { left: number; width: number })[] {
  const plan = bandId ? BAND_PLANS[bandId] : undefined;
  if (!plan || ve <= vs) return [];
  const span = ve - vs;
  return plan
    .filter((s) => s.hi > vs && s.lo < ve)
    .map((s) => {
      const a = Math.max(s.lo, vs);
      const b = Math.min(s.hi, ve);
      return { ...s, left: ((a - vs) / span) * 100, width: ((b - a) / span) * 100 };
    });
}

/** Where a click on a block tunes: its middle, or the middle of what shows of it. */
export function segmentTarget(s: Segment, vs: number, ve: number): number {
  return Math.round((Math.max(s.lo, vs) + Math.min(s.hi, ve)) / 2);
}
