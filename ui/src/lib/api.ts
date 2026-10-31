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
  /** Station data of the tuned FM station, once any has been decoded. */
  rds: Rds | null;
  sondes: Sonde[];
  /** Frequencies the balloon decoders are trying. */
  decoding_hz: number[];
  signals: Detected[];
  /** The tuned FM station shows HD Radio sidebands. */
  hd_sidebands: boolean;
  /** HD Radio decoding of the tuned station, while it is on. */
  hd: Hd | null;
  /** HD pictures; only present on the status that follows a change. */
  hd_images: HdImage[] | null;
}
/** HD Radio decoder state. `program` 0 is HD1. */
export interface Hd {
  error: string | null;
  program: number;
  programs: number[];
  synced: boolean;
  digital: boolean;
  name: string | null;
  slogan: string | null;
  message: string | null;
  title: string | null;
  artist: string | null;
  album: string | null;
  mer_db: number | null;
  ber: number | null;
  images_rev: number;
}
/** `kind`: logo, art, traffic (one tile of a 3×3 map, `row`/`col` from 1) or weather. */
export interface HdImage {
  kind: 'logo' | 'art' | 'traffic' | 'weather';
  name: string;
  row: number;
  col: number;
  data_url: string;
}

/** RDS station data. `ps` is 8 characters and often scrolls; `call` comes from the PI code. */
export interface Rds {
  pi: number | null;
  call: string | null;
  ps: string | null;
  radiotext: string | null;
  pty: string | null;
  title: string | null;
  artist: string | null;
}

/** A weather balloon heard by the decoders. Position fields are null until a GPS fix arrives. */
export interface Sonde {
  kind: string;
  serial: string;
  hz: number;
  frame: number | null;
  time: string | null;
  lat: number | null;
  lon: number | null;
  alt_m: number | null;
  speed_ms: number | null;
  heading_deg: number | null;
  climb_ms: number | null;
  frames: number;
  age_s: number;
}

/** A space station pass; times are Unix seconds. */
export interface IssPass {
  rise: number;
  top: number;
  set: number;
  max_elevation: number;
}

export interface Iss {
  /** The location text is not a ZIP code, state or "lat, lon". */
  bad_location: boolean;
  /** The location is a whole state, so times are rougher. */
  approximate: boolean;
  /** Unix seconds of the orbit data's epoch; null before the first download. */
  tle_epoch: number | null;
  passes: IssPass[];
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
/** Run the weather-balloon decoders on every narrow signal in the capture. */
export const setSondes = (on: boolean) => invoke('set_sondes', { on });
/** Decode HD Radio on the tuned FM station: 0 = HD1, null = off. */
export const setHd = (program: number | null) => invoke('set_hd', { program });
export const setSquelch = (db: number | null) => invoke('set_squelch', { db });

/** Space station passes over a ZIP code, state or "lat, lon" for the next two days. */
export const iss = (location: string) => invoke<Iss>('iss', { location });

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
/** Fresh orbit data was downloaded. */
export const onIss = (f: () => void) => listen('iss', () => f());
