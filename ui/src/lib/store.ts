// What Squelch keeps, in files the shell owns: settings.toml and schedule.toml in the config directory,
// and state.json (this machine's memory) in the state directory. `load` runs before the page mounts, so
// the app reads these synchronously. Saves wait a moment, so a drag writes once.
import * as api from './api';

const SETTINGS_WAIT_MS = 500;
const STATE_WAIT_MS = 1000;

let settings: api.Settings = {};
let settingsTimer: ReturnType<typeof setTimeout> | undefined;
let state: Record<string, unknown> = {};
let stateTimer: ReturnType<typeof setTimeout> | undefined;
let firstSettings: api.SettingsFile = { settings: {}, path: null, error: null };
let schedule: api.ScheduleFile = { slots: [], path: null, error: null };
let problem: (message: string) => void = console.error;

export async function load() {
  try {
    const [f, s, raw] = await Promise.all([api.settings(), api.schedule(), api.readState()]);
    settings = f.settings;
    firstSettings = f;
    schedule = s;
    state = parseState(raw);
  } catch (e) {
    firstSettings = { ...firstSettings, error: String(e) };
  }
  window.addEventListener('pagehide', flush);
}

export function parseState(raw: string): Record<string, unknown> {
  try {
    const v = JSON.parse(raw);
    return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
  } catch {
    return {};
  }
}

export const startupSchedule = () => schedule;
export const startupSettings = () => firstSettings;
export const onProblem = (f: (message: string) => void) => (problem = f);

export const getSettings = (): Readonly<api.Settings> => settings;

export function setSetting<K extends keyof api.Settings>(key: K, value: api.Settings[K]) {
  if ((settings[key] ?? null) === (value ?? null)) return;
  settings = { ...settings, [key]: value ?? null };
  clearTimeout(settingsTimer);
  settingsTimer = setTimeout(saveSettings, SETTINGS_WAIT_MS);
}

function saveSettings() {
  settingsTimer = undefined;
  api.saveSettings(settings).catch((e) => problem(String(e)));
}

/** Take what the file now says. Returns false while a change of ours waits to be written. */
export function adoptSettings(next: api.Settings): boolean {
  if (settingsTimer !== undefined) return false;
  settings = next;
  return true;
}

export function getState<T>(key: string, fallback: T): T {
  return key in state ? (state[key] as T) : fallback;
}

export function setState(key: string, value: unknown) {
  state[key] = value;
  stateTimer ??= setTimeout(saveState, STATE_WAIT_MS);
}

function saveState() {
  stateTimer = undefined;
  api.saveState(JSON.stringify(state)).catch((e) => console.error('state file', e));
}

function flush() {
  if (settingsTimer !== undefined) {
    clearTimeout(settingsTimer);
    saveSettings();
  }
  if (stateTimer !== undefined) {
    clearTimeout(stateTimer);
    saveState();
  }
}
