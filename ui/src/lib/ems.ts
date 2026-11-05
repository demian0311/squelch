// EMS is two stored bands far apart. The rail shows one entry, and a strip above the waterfall shows both side by side.
import type { Band, BandPlan, Status } from './api';

/** The bands behind the one EMS entry: the two P25 trunking bands and the two plain-FM bands fire and EMS still use. */
export const EMS_IDS = ['ps700', 'ps800', 'psvhf', 'psuhf'];
/** Rows the strip keeps; the engine sends one about every 30 s (`look::EVERY_SECS`), so about four minutes. */
export const STRIP_ROWS = 8;
/** Pixels between the two bands, where the 76 MHz between them is closed up. */
export const STRIP_GAP = 10;
/** A cell this far above the floor on the waterfall's byte scale counts as a signal to snap to: about 15 dB. */
const SNAP_MIN = 100;
/** A click this close to a signal means that signal. */
const SNAP_PX = 5;

export type LookLine = NonNullable<NonNullable<Status['look']>['line']>;
export type Section = { lo: number; hi: number; x: number; w: number };

export const isEms = (b: { id: string } | undefined | null) => !!b && EMS_IDS.includes(b.id);

/** The rail shows one EMS entry, under the first stored band's id. */
export function oneEms(plan: BandPlan): BandPlan {
  const first = plan.bands.find(isEms);
  if (!first) return plan;
  const bands = plan.bands.filter((b) => !isEms(b) || b === first).map((b) => (b === first ? { ...b, name: 'EMS', sub: '700 and 800 MHz' } : b));
  return { ...plan, bands };
}

/** The rail's entry for a stored band. */
export const railId = (plan: BandPlan, id: string | null) => (id && EMS_IDS.includes(id) ? (plan.bands.find(isEms)?.id ?? id) : id);

/** The ranges to check, lowest first, while an EMS band is open. */
export function emsRanges(plan: BandPlan, band: Band | undefined): [number, number][] | null {
  if (!isEms(band)) return null;
  return plan.bands
    .filter(isEms)
    .map((b): [number, number] => [b.low_mhz * 1e6, b.high_mhz * 1e6])
    .sort((a, b) => a[0] - b[0]);
}

/** Lays the ranges out in `width` pixels: width follows MHz, the gap between them closed up. */
export function stripSections(ranges: [number, number][], width: number, gap = STRIP_GAP): Section[] {
  const hz = ranges.reduce((s, [lo, hi]) => s + hi - lo, 0);
  const usable = Math.max(0, width - gap * (ranges.length - 1));
  let x = 0;
  return ranges.map(([lo, hi]) => {
    const w = ((hi - lo) / hz) * usable;
    const s = { lo, hi, x, w };
    x += w + gap;
    return s;
  });
}

export function stripX(sections: Section[], hz: number): number | null {
  const s = sections.find((a) => hz >= a.lo && hz <= a.hi);
  return s ? s.x + ((hz - s.lo) / (s.hi - s.lo)) * s.w : null;
}

/** The frequency under a pixel; `null` in the gap between the bands. */
export function stripHz(sections: Section[], x: number): number | null {
  const s = sections.find((a) => x >= a.x && x <= a.x + a.w);
  return s ? s.lo + ((x - s.x) / s.w) * (s.hi - s.lo) : null;
}

/** A line's cells for one section, one value per pixel: the strongest in it, so a narrow signal never falls between pixels. */
export function cellsToPixels(cells: number[], pixels: number): Uint8Array {
  const out = new Uint8Array(pixels);
  if (!cells.length) return out;
  const per = cells.length / pixels;
  for (let px = 0; px < pixels; px++) {
    const a = Math.floor(px * per);
    const b = Math.max(a + 1, Math.floor((px + 1) * per));
    for (let c = a; c < b && c < cells.length; c++) if (cells[c] > out[px]) out[px] = cells[c];
  }
  return out;
}

/** What a click on the strip means: the strongest signal near it in any kept line, else the frequency itself. */
export function stripPick(lines: LookLine[], hz: number, hzPerPx: number): number {
  const reach = SNAP_PX * hzPerPx;
  let best = hz;
  let top = SNAP_MIN - 1;
  for (const line of lines) {
    for (const p of line.parts) {
      if (hz < p.lo_hz || hz > p.hi_hz) continue;
      const per = (p.hi_hz - p.lo_hz) / p.cells.length;
      const a = Math.max(0, Math.floor((hz - reach - p.lo_hz) / per));
      const b = Math.min(p.cells.length - 1, Math.floor((hz + reach - p.lo_hz) / per));
      for (let c = a; c <= b; c++) {
        if (p.cells[c] > top) {
          top = p.cells[c];
          best = p.lo_hz + (c + 0.5) * per;
        }
      }
    }
  }
  return best;
}

/** Adds a line once (a line rides in several statuses), newest first, keeping `STRIP_ROWS`. */
export function addLine(lines: LookLine[], line: LookLine | null | undefined): LookLine[] {
  if (!line || lines[0]?.seq === line.seq) return lines;
  return [line, ...lines].slice(0, STRIP_ROWS);
}

/** The strip's words: what it is doing and how old the picture is. */
export function stripSay(looking: boolean, lines: number, agoS: number | null): string {
  if (looking) return 'Checking the whole band. Live is paused for a moment.';
  if (!lines || agoS === null) return 'The whole band appears here after its first check.';
  return `Whole band checked ${agoS < 60 ? `${Math.round(agoS)} s` : `${Math.round(agoS / 60)} min`} ago. It waits while a call plays.`;
}

/** A carrier heard in this share of the kept lines or fewer comes and goes like voice; above it, it is always on (a control channel, data, paging). */
const VOICE_MAX_SHARE = 0.75;
/** Lines needed before a carrier can be called voice. */
const VOICE_MIN_LINES = 3;
/** The strip keeps rows this far apart in time (`look::EVERY_SECS`). */
export const LINE_SECS = 30;
/** Carriers listed, strongest pattern first. */
const VOICE_MAX = 12;

export interface VoiceHeard {
  hz: number;
  /** Lines it was on, of those kept. */
  seen: number;
  of: number;
  /** Lines ago it was last on: 0 is the newest line. */
  ago: number;
  /** Strongest reading, on the waterfall's byte scale. */
  level: number;
}

/**
 * Plain voice found by the look: carriers above `SNAP_MIN` in some kept lines but not most of them. A control channel,
 * a data channel or a pager is on in every line; a dispatcher keys up for seconds and is caught in some looks only.
 * Neighbouring cells of one carrier fold into the strongest. Newest line first in `lines`.
 */
export function voiceHeard(lines: LookLine[]): VoiceHeard[] {
  if (lines.length < VOICE_MIN_LINES) return [];
  const of = lines.length;
  const out: VoiceHeard[] = [];
  const newest = lines[0];
  newest.parts.forEach((part, pi) => {
    const per = (part.hi_hz - part.lo_hz) / part.cells.length;
    // per cell: in how many lines, the newest line it was on, the strongest reading
    const seen = new Array<number>(part.cells.length).fill(0);
    const last = new Array<number>(part.cells.length).fill(-1);
    const level = new Array<number>(part.cells.length).fill(0);
    lines.forEach((line, li) => {
      const p = line.parts[pi];
      if (!p || p.cells.length !== part.cells.length) return;
      p.cells.forEach((v, c) => {
        if (v < SNAP_MIN) return;
        seen[c]++;
        if (last[c] < 0) last[c] = li;
        if (v > level[c]) level[c] = v;
      });
    });
    // one carrier spans a cell or two: a run of hit cells is one carrier, at its strongest cell
    let c = 0;
    while (c < seen.length) {
      if (!seen[c]) {
        c++;
        continue;
      }
      let best = c;
      let end = c;
      while (end < seen.length && seen[end]) {
        if (seen[end] > seen[best] || (seen[end] === seen[best] && level[end] > level[best])) best = end;
        end++;
      }
      if (seen[best] <= of * VOICE_MAX_SHARE) out.push({ hz: part.lo_hz + (best + 0.5) * per, seen: seen[best], of, ago: last[best], level: level[best] });
      c = end;
    }
  });
  return out.sort((a, b) => b.seen - a.seen || a.ago - b.ago || b.level - a.level).slice(0, VOICE_MAX);
}
