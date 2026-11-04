// EMS is two stored bands far apart. The rail shows one entry, and a strip above the waterfall shows both side by side.
import type { Band, BandPlan, Status } from './api';

export const EMS_IDS = ['ps700', 'ps800'];
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
