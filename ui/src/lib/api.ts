// Calls into the Rust side, and the shapes it sends back.
import { invoke } from '@tauri-apps/api/core';
import { bytesFromBase64, decodeFeed } from './feed';
import { listen } from '@tauri-apps/api/event';

export type Mode = 'wfm' | 'nfm' | 'am' | 'usb' | 'lsb' | 'none' | 'p25';

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

/** One line of the user's repeaters file, from RepeaterBook's KML export. */
export interface Repeater {
  call: string;
  city: string;
  mhz: number;
  lat: number;
  lon: number;
  /** "+", "-", "s" for a non-standard split, or "" */
  shift: string;
  /** CTCSS Hz ("123.0"), DCS ("D023"), "CSQ", or "" */
  tone: string;
  modes: string[];
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
  /** Trunked radio receiver, in P25 mode. */
  p25: P25 | null;
  /** Crystal correction applied now, ppm; positive means the crystal runs fast. */
  ppm: number;
  /** The crystal error last measured, on a strong FM station or a P25 control channel. */
  crystal: { ppm: number; hz: number } | null;
}
/** A talkgroup call granted by the control channel. */
export interface P25Call {
  group: number;
  /** Radio ID of the talker; 0 until a grant names it. */
  source: number;
  /** Voice channel, true frequency. */
  hz: number;
  encrypted: boolean;
  /** Phase 2 (TDMA) channel, which the receiver cannot play. */
  tdma: boolean;
  emergency: boolean;
  /** Receiver clock (`P25.now_secs`) when granted. */
  at_secs: number;
}
export interface P25 {
  state: 'searching' | 'control' | 'voice';
  now_secs: number;
  nac: number | null;
  wacn: number | null;
  system: number | null;
  rfss: number | null;
  site: number | null;
  control_hz: number | null;
  /** Dongle frequency error measured on the control channel. */
  ppm: number | null;
  messages_per_sec: number;
  call: P25Call | null;
  recent: P25Call[];
  /** Why voice cannot play (mbelib missing). */
  voice_error: string | null;
  voice_errors_pct: number;
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

/** A point under the station; `t` is Unix seconds. */
export interface GroundPoint {
  t: number;
  lat: number;
  lon: number;
  alt_km: number;
}

export interface Iss {
  /** The location text is not a ZIP code, state or "lat, lon". */
  bad_location: boolean;
  /** The location is a whole state, so times are rougher. */
  approximate: boolean;
  site: { lat: number; lon: number } | null;
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
export const repeaters = (location: string) => invoke<Repeater[]>('repeaters', { location });
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
/** Correct the dongle's crystal by `ppm`, positive when it runs fast. */
export const setPpm = (ppm: number) => invoke('set_ppm', { ppm });

/** Space station passes over a ZIP code, state or "lat, lon" for the next two days. */
export const iss = (location: string) => invoke<Iss>('iss', { location });

/** The station's ground track, 45 min back to 100 min ahead. */
export const issTrack = () => invoke<GroundPoint[]>('iss_track');
/** One elevation tile (terrarium PNG) as a data URL, or null when neither cached nor downloadable. */
export const terrainTile = (z: number, x: number, y: number) => invoke<string | null>('terrain_tile', { z, x, y });
/** A shaded local map saved on disk under `key`, or null. */
export const ground = (key: string) => invoke<string | null>('ground', { key });
export const saveGround = (key: string, dataUrl: string) => invoke<void>('save_ground', { key, dataUrl });

let handlers: { onRow: (r: Row) => void; onStatus: (s: Status) => void } | null = null;

/** Start the engine, then pull its rows and status one request at a time: a page that falls behind skips rows instead of queueing them. */
export async function start(onRow: (r: Row) => void, onStatus: (s: Status) => void) {
  const pulling = handlers !== null;
  handlers = { onRow, onStatus };
  await invoke('start');
  if (!pulling) pull();
}

async function pull() {
  for (;;) {
    const b64 = await invoke<string>('feed').catch(() => null);
    if (b64 === null) {
      await new Promise((r) => setTimeout(r, 500));
      continue;
    }
    const { status, rows } = decodeFeed<Status>(bytesFromBase64(b64));
    if (status) handlers?.onStatus(status);
    for (const r of rows) handlers?.onRow(r);
  }
}

export const onTheme = (f: (t: Theme) => void) => listen<Theme>('theme', (e) => f(e.payload));
export const onRepeaters = (f: () => void) => listen('repeaters', () => f());
export const onBands = (f: (b: Bands) => void) => listen<Bands>('bands', (e) => f(e.payload));
/** Fresh orbit data was downloaded. */
export const onIss = (f: () => void) => listen('iss', () => f());
