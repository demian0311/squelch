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

/** Colour stops for the waterfall, from the noise floor up to the strongest signal. */
export function waterfallStops(t: Theme): string[] {
  const base = t.darker_background;
  return [base, mix(base, t.accent, 0.35), t.accent, t.cyan, t.yellow, t.bright_foreground];
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
    '--cyan': t.cyan,
    '--red': t.red,
  };
  for (const [k, val] of Object.entries(v)) r.setProperty(k, val);
  r.setProperty('color-scheme', t.mode === 'light' ? 'light' : 'dark');
}
