// Maps the Omarchy colours file onto CSS variables and the waterfall colour scale.
import type { Theme } from './api';

// Slate Dark, used when no Omarchy theme is found
const FALLBACK: Theme = {
  mode: 'dark',
  accent: '#5b9bd5',
  selection: '#29323e',
  muted: '#38424f',
  background: '#161b22',
  dark_background: '#11151b',
  darker_background: '#0c1014',
  lighter_background: '#202833',
  foreground: '#e6eaef',
  dark_foreground: '#6e7a87',
  light_foreground: '#9aa5b1',
  bright_foreground: '#f4f7fa',
  red: '#e07b6e',
  yellow: '#d9bd5a',
  orange: '#e0975a',
  green: '#74b56e',
  cyan: '#45b3a3',
  blue: '#5b9bd5',
  magenta: '#a585c9',
};

const HEX = /^#[0-9a-f]{6}$/i;

export function resolve(t: Theme): Theme {
  const out: Theme = { ...FALLBACK };
  for (const [k, v] of Object.entries(t)) if (k === 'mode' || k === 'name' || HEX.test(v)) out[k] = v;
  return out;
}

export function rgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function mix(a: string, b: string, t: number): string {
  const x = rgb(a);
  const y = rgb(b);
  return '#' + x.map((v, i) => Math.round(v + (y[i] - v) * t).toString(16).padStart(2, '0')).join('');
}

// Each band group wears one theme colour, so a glance says which kind of band is tuned.
const GROUP_HUES: Record<string, keyof Theme> = { listen: 'blue', safety: 'red', ham: 'green', data: 'orange', space: 'magenta' };
const SPARE_HUES: (keyof Theme)[] = ['cyan', 'yellow', 'blue', 'green', 'orange', 'magenta', 'red'];

/** The colour of a band group; groups from a user's bands file take spare colours in order. */
export function groupHue(t: Theme, groupId: string | undefined, groupIds: string[]): string {
  if (!groupId) return t.accent;
  const named = GROUP_HUES[groupId];
  if (named) return t[named] ?? t.accent;
  const spare = groupIds.filter((g) => !GROUP_HUES[g]).indexOf(groupId);
  return t[SPARE_HUES[Math.max(0, spare) % SPARE_HUES.length]] ?? t.accent;
}

/** Colour stops for the waterfall, from 8 dB under the noise floor (byte 0) to 60 dB over it (255): the classic receiver
 * ramp (gqrx, SDR++) in the theme's colours, so strength reads as hue. Spread evenly, noise (bytes ~25-45) sits in the
 * dark blue, a weak station reaches blue, a strong FM station (~byte 160) orange, and only a very strong one goes white. */
export function waterfallStops(t: Theme): string[] {
  const base = t.darker_background;
  return [base, mix(base, t.blue, 0.5), t.blue, t.cyan, t.yellow, t.orange, t.red, mix(t.red, t.bright_foreground, 0.5), t.bright_foreground];
}

/** 256 RGB entries spread across the stops. */
export function lut(stops: string[]): Uint8ClampedArray {
  const cs = stops.map(rgb);
  const out = new Uint8ClampedArray(256 * 3);
  for (let i = 0; i < 256; i++) {
    const p = (i / 255) * (cs.length - 1);
    const k = Math.min(cs.length - 2, Math.floor(p));
    const f = p - k;
    for (let j = 0; j < 3; j++) out[i * 3 + j] = cs[k][j] + (cs[k + 1][j] - cs[k][j]) * f;
  }
  return out;
}

export function apply(t: Theme) {
  const r = document.documentElement.style;
  const v: Record<string, string> = {
    '--bg': t.background,
    '--bg-dark': t.dark_background,
    '--bg-darker': t.darker_background,
    '--bg-light': t.lighter_background,
    '--sel': t.selection,
    '--muted': t.muted,
    '--fg': t.foreground,
    '--fg-dim': t.light_foreground,
    '--fg-bright': t.bright_foreground,
    '--accent': t.accent,
    '--green': t.green,
    '--yellow': t.yellow,
    '--orange': t.orange,
    '--blue': t.blue,
    '--magenta': t.magenta,
    '--cyan': t.cyan,
    '--red': t.red,
  };
  for (const [k, val] of Object.entries(v)) r.setProperty(k, val);
  r.setProperty('color-scheme', t.mode === 'light' ? 'light' : 'dark');
}
