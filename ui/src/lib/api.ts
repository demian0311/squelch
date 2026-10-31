// Calls into the Rust side, and the shapes it sends back.
import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type Mode = 'wfm' | 'nfm' | 'am' | 'usb' | 'lsb' | 'none';

export interface Bookmark {
  mhz: number;
  name: string;
  mode?: Mode | null;
}

export interface Band {
  id: string;
  group: string;
  name: string;
  icon: string;
  mode: Mode;
  low_mhz: number;
  high_mhz: number;
  start_mhz: number;
  decoder?: string | null;
  bookmarks: Bookmark[];
}

export interface BandPlan {
  groups: { id: string; name: string }[];
  bands: Band[];
}

export interface Bands {
  plan: BandPlan;
  path: string | null;
  error: string | null;
}

export interface Detected {
  hz: number;
  width_hz: number;
  snr_db: number;
}

export interface Status {
  device: string;
  simulated: boolean;
  error: string | null;
  sample_rate: number;
  center_hz: number;
  tuned_hz: number;
  mode: Mode;
  playing: boolean;
  gains: number[];
  floor_db: number;
  channel_snr_db: number;
  squelch_db: number | null;
  squelch_open: boolean;
  signals: Detected[];
}

export interface Setup {
  sample_rate: number;
  fft_size: number;
  below_floor_db: number;
  range_db: number;
}

export interface Row {
  centerHz: number;
  bins: Uint8Array;
}

export type Theme = Record<string, string>;

export const setup = () => invoke<Setup>('setup');
export const bands = () => invoke<Bands>('bands');
export const theme = () => invoke<Theme>('theme');
export const tune = (centerHz: number, tunedHz: number) => invoke('tune', { centerHz, tunedHz });
export const setMode = (mode: Mode) => invoke('set_mode', { mode });
export const setGain = (tenths: number | null) => invoke('set_gain', { tenths });
export const setVolume = (volume: number) => invoke('set_volume', { volume });
export const setPlaying = (playing: boolean) => invoke('set_playing', { playing });
/** Mute below `db` above the noise floor; null turns squelch off. */
export const setSquelch = (db: number | null) => invoke('set_squelch', { db });

export function start(onRow: (r: Row) => void, onStatus: (s: Status) => void) {
  const rows = new Channel<ArrayBuffer>();
  rows.onmessage = (buf) => {
    const view = new DataView(buf);
    onRow({ centerHz: view.getFloat64(0, true), bins: new Uint8Array(buf, 8) });
  };
  const status = new Channel<Status>();
  status.onmessage = onStatus;
  return invoke('start', { rows, status });
}

export const onTheme = (f: (t: Theme) => void) => listen<Theme>('theme', (e) => f(e.payload));
export const onBands = (f: (b: Bands) => void) => listen<Bands>('bands', (e) => f(e.payload));
