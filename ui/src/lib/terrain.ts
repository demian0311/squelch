// The local map's ground: shaded terrain from elevation tiles, plus state lines and highways, in km around the listener.
// Same flat projection as repeaterPoints: x east, y south, km.

export const TILE = 256;
const EARTH_KM = 6371;
const rad = (d: number) => (d * Math.PI) / 180;

export interface Site {
  lat: number;
  lon: number;
}
export interface Frame {
  site: Site;
  halfWKm: number;
  halfHKm: number;
}

const kmPerLon = (s: Site) => (EARTH_KM * Math.PI * Math.cos(rad(s.lat))) / 180;
const KM_PER_LAT = (EARTH_KM * Math.PI) / 180;

export function toKm(s: Site, lon: number, lat: number): [number, number] {
  return [(lon - s.lon) * kmPerLon(s), (s.lat - lat) * KM_PER_LAT];
}
export function fromKm(s: Site, x: number, y: number): [number, number] {
  return [s.lon + x / kmPerLon(s), s.lat - y / KM_PER_LAT];
}

/** Web Mercator pixel at zoom `z` (tile size 256). */
export function mercator(lon: number, lat: number, z: number): [number, number] {
  const n = TILE * 2 ** z;
  const r = rad(Math.max(-85, Math.min(85, lat)));
  return [((lon + 180) / 360) * n, ((1 - Math.log(Math.tan(r) + 1 / Math.cos(r)) / Math.PI) / 2) * n];
}

/** Tiles at zoom `z` that cover the frame. */
export function tilesFor(f: Frame, z: number): { x: number; y: number }[] {
  const [w, n] = fromKm(f.site, -f.halfWKm, -f.halfHKm);
  const [e, s] = fromKm(f.site, f.halfWKm, f.halfHKm);
  const [x0, y0] = mercator(w, n, z).map((v) => Math.floor(v / TILE));
  const [x1, y1] = mercator(e, s, z).map((v) => Math.floor(v / TILE));
  const out = [];
  for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) out.push({ x, y });
  return out;
}

/** Metres from a terrarium pixel. */
export const terrarium = (r: number, g: number, b: number) => r * 256 + g + b / 256 - 32768;

export interface Palette {
  /** low flat ground */
  plain: string;
  /** high ground */
  peak: string;
  /** slopes facing away from the light */
  shadow: string;
  /** slopes facing the light */
  light: string;
}

function rgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

// light from the north-west, 45° up; relief exaggerated so plains still show texture
const AZIMUTH = rad(315);
const ALTITUDE = rad(45);
const EXAGGERATION = 2.2;
const LIGHT = [Math.sin(AZIMUTH) * Math.cos(ALTITUDE), Math.cos(AZIMUTH) * Math.cos(ALTITUDE), Math.sin(ALTITUDE)];
// heights that map to `plain` and `peak`
const LOW_M = 1200;
const HIGH_M = 4200;

/**
 * Shaded relief for a `w` × `h` grid of heights in metres, `kmPerPx` apart. NaN heights (no tile) come out transparent.
 * Returns RGBA bytes.
 */
export function shade(heights: Float32Array, w: number, h: number, kmPerPx: number, p: Palette): Uint8ClampedArray<ArrayBuffer> {
  const [plain, peak, dark, lite] = [p.plain, p.peak, p.shadow, p.light].map(rgb);
  const out = new Uint8ClampedArray(new ArrayBuffer(w * h * 4));
  const step = kmPerPx * 1000;
  const flat = Math.sin(ALTITUDE);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      const e = heights[i];
      if (Number.isNaN(e)) continue;
      const at = (xx: number, yy: number) => {
        const v = heights[Math.min(h - 1, Math.max(0, yy)) * w + Math.min(w - 1, Math.max(0, xx))];
        return Number.isNaN(v) ? e : v;
      };
      const dzdx = ((at(x + 1, y) - at(x - 1, y)) / (2 * step)) * EXAGGERATION;
      const dzdy = ((at(x, y + 1) - at(x, y - 1)) / (2 * step)) * EXAGGERATION;
      // surface normal (east, north, up) against the light; the grid's y runs south
      const lit = Math.max(0, (-dzdx * LIGHT[0] + dzdy * LIGHT[1] + LIGHT[2]) / Math.hypot(dzdx, dzdy, 1));
      const t = Math.min(1, Math.max(0, (e - LOW_M) / (HIGH_M - LOW_M)));
      // flat ground keeps the ground colour; lit slopes lighten, shaded ones darken
      const d = (lit - flat) / flat;
      for (let j = 0; j < 3; j++) {
        const g = plain[j] + (peak[j] - plain[j]) * t;
        out[i * 4 + j] = d > 0 ? g + (lite[j] - g) * Math.min(1, d * 1.6) * 0.55 : g + (dark[j] - g) * Math.min(1, -d * 1.6) * 0.9;
      }
      out[i * 4 + 3] = 255;
    }
  }
  return out;
}

export interface Lines {
  states: [number, number][][];
  roads: { major: number; c: [number, number][] }[];
}

/** SVG path data in km for the lines that cross the frame. */
export function linePaths(lines: Lines, f: Frame): { states: string; major: string; minor: string } {
  const margin = 1.2;
  const inView = ([x, y]: [number, number]) => Math.abs(x) <= f.halfWKm * margin && Math.abs(y) <= f.halfHKm * margin;
  const path = (line: [number, number][]) => {
    const pts = line.map(([lon, lat]) => toKm(f.site, lon, lat));
    if (!pts.some(inView)) return '';
    return 'M' + pts.map(([x, y]) => `${x.toFixed(1)} ${y.toFixed(1)}`).join('L');
  };
  return {
    states: lines.states.map(path).join(''),
    major: lines.roads.filter((r) => r.major).map((r) => path(r.c)).join(''),
    minor: lines.roads.filter((r) => !r.major).map((r) => path(r.c)).join(''),
  };
}
