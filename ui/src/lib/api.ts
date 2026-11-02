// Calls into the Rust side, and the shapes it sends back.
import { invoke } from '@tauri-apps/api/core';
import { bytesFromBase64, decodeFeed } from './feed';
import { listen } from '@tauri-apps/api/event';
import type { ReadSlot, Slot } from './schedule';

export type Mode = 'wfm' | 'nfm' | 'am' | 'usb' | 'lsb' | 'none' | 'p25';

export interface Bookmark {
  mhz: number;
  name: string;
  mode?: Mode | null;
  /** Free text about the frequency. */
  notes?: string | null;
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
  /** What frequencies are for, known to the app; nobody saved them. */
  names?: { mhz: number; name: string; mode?: Mode | null }[];
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
  /** RepeaterBook's own, unique even when two share a frequency */
  id: string;
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
  /** Aircraft decoder, while it runs. */
  adsb: Adsb | null;
  signals: Detected[];
  /** The tuned FM station shows HD Radio sidebands. */
  hd_sidebands: boolean;
  /** HD Radio decoding of the tuned station, while it is on. */
  hd: Hd | null;
  /** HD pictures; only present on the status that follows a change. */
  hd_images: HdImage[] | null;
  /** Trunked radio receiver, in P25 mode. */
  p25: P25 | null;
  /** Aircraft messages, while a band with the ACARS decoder is selected. */
  acars: Acars | null;
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
  /** How far along the furthest logo or album art still arriving is, 0–1. */
  incoming: number | null;
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

/** An ACARS block. Downlinks (block id a digit) carry `msn` and `flight`; the others leave them empty. */
export interface AcarsMessage {
  /** Counts up from 1 each time decoding starts. */
  id: number;
  at_ms: number;
  /** The channel it was heard on. */
  hz: number;
  mode: string;
  registration: string;
  /** The block it acknowledges; null is NAK. */
  ack: string | null;
  label: string;
  block_id: string;
  /** More blocks of the same message follow. */
  more: boolean;
  msn: string;
  flight: string;
  text: string;
}

export interface Acars {
  /** Channels being decoded now. */
  channels: number[];
  /** Messages of the last few seconds, oldest first; the panel keeps its own list. */
  messages: AcarsMessage[];
  /** Blocks decoded since decoding started. */
  heard: number;
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

/** An aircraft heard on 1090 MHz. Fields are null until a message carrying them arrives. */
export interface Aircraft {
  /** 24-bit address, six hex digits. */
  icao: string;
  callsign: string | null;
  /** Pressure altitude. */
  alt_ft: number | null;
  speed_kt: number | null;
  /** Track over the ground, or heading when `airspeed`. */
  heading_deg: number | null;
  airspeed: boolean;
  vrate_fpm: number | null;
  lat: number | null;
  lon: number | null;
  /** Seconds since the last message, and since the last position. */
  age_s: number;
  pos_age_s: number | null;
  messages: number;
  snr_db: number;
}
export interface Adsb {
  aircraft: Aircraft[];
  frames_per_sec: number;
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

/** settings.toml; a missing key takes the app's default. */
export interface Settings {
  location?: string | null;
  squelch?: number | null;
  /** Set by hand; without it the measured error applies. */
  crystal_ppm?: number | null;
  show_dead_air?: boolean | null;
  /** `cpu` forces the 2D renderer, to check it on a machine that has WebGL. */
  waterfall?: string | null;
}

export interface SettingsFile {
  settings: Settings;
  path: string | null;
  error: string | null;
}

export interface ScheduleFile {
  slots: ReadSlot[];
  path: string | null;
  error: string | null;
}

export const setup = () => invoke<Setup>('setup');
export const settings = () => invoke<SettingsFile>('settings');
export const saveSettings = (settings: Settings) => invoke<void>('save_settings', { settings });
export const schedule = () => invoke<ScheduleFile>('schedule');
export const saveSchedule = (slots: Slot[]) => invoke<ScheduleFile>('save_schedule', { slots });
/** Wake the computer at this time (seconds since the epoch), or at no time. Rejects with why it can't. */
export const setWake = (at: number | null) => invoke<void>('set_wake', { at });
/** The command that lets Squelch set the wake alarm, ready to paste in a terminal. */
export const wakeSetupCommand = () => invoke<string>('wake_setup_command');
/** Keep the computer awake for this many seconds from now; 0 lets it go. */
export const holdAwake = (secs: number) => invoke<void>('hold_awake', { secs });
export const readState = () => invoke<string>('read_state');
export const saveState = (json: string) => invoke<void>('save_state', { json });
export const bands = () => invoke<Bands>('bands');
export const repeaters = (location: string) => invoke<Repeater[]>('repeaters', { location });
export const repeatersMap = (location: string) => invoke<Repeater[]>('repeaters_map', { location });
export const theme = () => invoke<Theme>('theme');
export const tune = (centerHz: number, tunedHz: number) => invoke('tune', { centerHz, tunedHz });
export const setMode = (mode: Mode) => invoke('set_mode', { mode });
export const setGain = (tenths: number | null) => invoke('set_gain', { tenths });
export const setVolume = (volume: number) => invoke('set_volume', { volume });
/** Where recording stands: the file being written, or the last one finished. */
export interface Recording {
  recording: boolean;
  file: string | null;
  secs: number;
  error: string | null;
}
export const record = (on: boolean) => invoke('record', { on });
export const recording = () => invoke<Recording>('recording');
export const setPlaying = (playing: boolean) => invoke('set_playing', { playing });
/** Mute below `db` above the noise floor; null turns squelch off. */
/** Run the weather-balloon decoders on every narrow signal in the capture. */
export const setSondes = (on: boolean) => invoke('set_sondes', { on });
/** Decode ACARS on the US channels in the capture. */
export const setAcars = (on: boolean) => invoke('set_acars', { on });
/** Decode aircraft broadcasts across the capture. */
export const setAdsb = (on: boolean) => invoke('set_adsb', { on });
/** Decode HD Radio on the tuned FM station: 0 = HD1, null = off. */
export const setHd = (program: number | null) => invoke('set_hd', { program });
export const setSquelch = (db: number | null) => invoke('set_squelch', { db });
/** Correct the dongle's crystal by `ppm`, positive when it runs fast. */
export const setPpm = (ppm: number) => invoke('set_ppm', { ppm });

/** Add or change a bookmark in the bands file (found by frequency within band `band`); resolves to the list as now read. */
export const saveBookmark = (band: string, bookmark: Bookmark) => invoke<Bands>('save_bookmark', { band, bookmark });
export const deleteBookmark = (band: string, mhz: number) => invoke<Bands>('delete_bookmark', { band, mhz });
/** A desktop notification, if the system has one. */
export const notify = (title: string, body: string) => invoke<void>('notify', { title, body });

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
export const onSettings = (f: (s: SettingsFile) => void) => listen<SettingsFile>('settings', (e) => f(e.payload));
export const onSchedule = (f: (s: ScheduleFile) => void) => listen<ScheduleFile>('schedule', (e) => f(e.payload));
export const onBands = (f: (b: Bands) => void) => listen<Bands>('bands', (e) => f(e.payload));
/** Fresh orbit data was downloaded. */
export const onIss = (f: () => void) => listen('iss', () => f());
