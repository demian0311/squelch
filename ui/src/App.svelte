<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import * as api from './lib/api';
  import { channelAt, channelList, quietChannels, setRepeaters } from './lib/channels';
  import type { Band, BandPlan, GroundPoint, Iss, Mode, Setup, Status, Theme } from './lib/api';
  import { allBookmarks, bandAt, bandEntryHz, fmtHz, fmtMHz, label, modeAt, Persistence, placeCards, anchor, withoutDeadAir, fmChannel, rdsName, rdsNow, snapTune, withRds, sondeDrift, sondeName, sondeSummary, withSondes, type Saved, type Signal } from './lib/radio';
  import { apply as applyTheme, groupHue, mix, rampGradient, resolve, waterfallStops } from './lib/theme';
  import { hdLabel, hdState, nextHd, trafficGrid } from './lib/hd';
  import { callAge, callBlock, p25Site, p25Summary } from './lib/p25';
  import { ageDays, ageLabel, clock, dayLabel, freshness, freshnessNote, groundAt, issRailLine, issTitle, localTime, utcTime, overhead, RECEPTION_DB, until, upcoming } from './lib/iss';
  import BandRail from './components/BandRail.svelte';
  import Waterfall from './components/Waterfall.svelte';
  import BandStrip from './components/BandStrip.svelte';
  import { noteHeard, stripTicks, type Heard } from './lib/stations';
  import Pane from './components/Pane.svelte';
  import IssMap from './components/IssMap.svelte';
  import RepeaterMap from './components/RepeaterMap.svelte';
  import Level from './components/Level.svelte';
  import { REACH_KM, repeaterPoints } from './lib/repeaters';
  import Icon from './components/Icon.svelte';

  const BANDWIDTH: Record<Mode, number> = { wfm: 200_000, nfm: 12_500, am: 8_000, usb: 2_800, lsb: 2_800, none: 10_000, p25: 12_500 };
  const CARD_W = 128;
  // HD signal quality (nrsc5's MER) mapped onto the level bar, empty to full
  const HD_MER_DB = [2, 14];
  const HD_AUDIO_HOLD_MS = 3_000;
  const LANE_H = 78;
  const SPEC_H = 130;
  // the waterfall's spectrum line while the panel has the screen
  const STRIP_SPEC_H = 24;
  const MIN_SPAN = 25_000;
  // a measurement past this is a bad one; cheap dongles are within about 100 ppm
  const MAX_PPM = 200;
  // the signal meter's full scale, dB above the noise floor
  const METER_DB = 40;
  // keep the listened-to frequency off the tuner's centre
  const DC_OFFSET = 60_000;

  let setup = $state<Setup>({ sample_rate: 2_400_000, fft_size: 2048, below_floor_db: 8, range_db: 60 });
  let plan = $state<BandPlan>({ groups: [], bands: [] });
  let bandsPath = $state<string | null>(null);
  let bandsError = $state<string | null>(null);
  let theme = $state<Theme>(resolve({}));
  let status = $state<Status | null>(null);

  let bandId = $state<string | null>(null);
  let vs = $state(100_200_000);
  let ve = $state(102_000_000);
  let center = $state(101_400_000);
  let tuned = $state(101_100_000);
  let mode = $state<Mode>('wfm');
  let playing = $state(true);
  // recording: what the engine says, and the moment it started on the clock below
  let rec = $state<api.Recording | null>(null);
  let recAt = $state(0);
  let volume = $state(0.7);
  let gain = $state<string>('auto');
  // crystal correction: measured unless typed in by hand
  let ppm = $state(readNumber('ppm', 0));
  let ppmByHand = $state(readNumber('ppm.hand', 0) === 1);
  let railOpen = $state(readNumber('rail', 1) === 1);
  let settingsOpen = $state(false);
  // dB above the noise floor; 0 means off. Empty channels read 3–10 dB on this dongle.
  let squelch = $state(readNumber('squelch', 12));

  function readNumber(key: string, fallback: number): number {
    try {
      const v = localStorage.getItem(key);
      return v === null ? fallback : +v;
    } catch {
      return fallback;
    }
  }
  // bands that rarely give us anything stay off the rail until someone wants to investigate them
  let showDeadAir = $state(readNumber('deadAir', 0) === 1);
  let railPlan = $derived(showDeadAir ? plan : withoutDeadAir(plan));
  function toggleDeadAir() {
    showDeadAir = !showDeadAir;
    try {
      localStorage.setItem('deadAir', showDeadAir ? '1' : '0');
    } catch {
      // remembering the choice is a nicety
    }
  }
  /** HD Radio program chosen per FM station (by channel Hz); a station without an entry plays analog. */
  let hdPrefs = $state<Record<string, number>>(readHdPrefs());
  function readHdPrefs(): Record<string, number> {
    try {
      return JSON.parse(localStorage.getItem('hd') ?? '{}');
    } catch {
      return {};
    }
  }
  /** Last frequency used in each band, so returning to a band picks up where it left off. */
  const lastHz: Record<string, number> = readLastHz();
  function readLastHz(): Record<string, number> {
    try {
      return JSON.parse(localStorage.getItem('lastHz') ?? '{}');
    } catch {
      return {};
    }
  }
  function remember(key: string, v: string) {
    try {
      localStorage.setItem(key, v);
    } catch {
      // remembering the choice is a nicety
    }
  }
  function toggleRail() {
    railOpen = !railOpen;
    remember('rail', railOpen ? '1' : '0');
  }
  let full = $state(false);
  let boxW = $state(1000);
  // while the view itself moves, cards jump with it; the slide is only for cards appearing or folding
  let panning = $state(false);
  let panTimer: ReturnType<typeof setTimeout> | undefined;
  let waterfall: Waterfall;
  /** Station data per FM channel, kept after tuning away. */
  let rdsHeard = $state(new Map<number, api.Rds>());
  /** Stations heard per band, across runs, for the band strip; localStorage `stations`. */
  let heard = $state<Record<string, Heard[]>>(readHeard());
  function readHeard(): Record<string, Heard[]> {
    try {
      return JSON.parse(localStorage.getItem('stations') ?? '{}');
    } catch {
      return {};
    }
  }
  /** Ticks heard before this are from an earlier run, and the station may be gone. */
  const startedAt = Date.now();
  const persist = new Persistence();
  // status arrives every 200 ms, so a 15 s window holds 75 updates
  const UPDATES_PER_WINDOW = 75;
  let showSteady = $state(false);

  let saved = $derived<Saved[]>(allBookmarks(plan));
  let band = $derived(plan.bands.find((b) => b.id === bandId));
  let rate = $derived(setup.sample_rate);
  let maxSpan = $derived(rate * 0.8);
  let span = $derived(ve - vs);

  // ── tuning ──────────────────────────────────────────

  let pending = 0;
  function send() {
    if (pending) return;
    pending = requestAnimationFrame(() => {
      pending = 0;
      api.tune(center, tuned);
    });
  }

  /** Move the hardware centre when the view or the tuned frequency leaves the usable capture. */
  function recenter() {
    const half = rate * 0.45;
    const inside = (f: number) => f >= center - half && f <= center + half;
    if (!inside(vs) || !inside(ve) || !inside(tuned)) {
      let c = (vs + ve) / 2;
      if (Math.abs(tuned - c) < DC_OFFSET) c = tuned + DC_OFFSET;
      center = Math.round(c);
    }
    send();
  }

  function setView(nvs: number, nve: number) {
    let s = Math.min(Math.max(nve - nvs, MIN_SPAN), maxSpan);
    const mid = (nvs + nve) / 2;
    nvs = Math.max(24e6, mid - s / 2);
    vs = nvs;
    ve = nvs + s;
    panning = true;
    clearTimeout(panTimer);
    panTimer = setTimeout(() => (panning = false), 200);
    recenter();
  }

  function tuneTo(hz: number, keepMode = false) {
    tuned = Math.round(hz);
    if (tuned < vs || tuned > ve) {
      vs = tuned - span / 2;
      ve = tuned + span / 2;
    }
    if (!keepMode) {
      const m = modeAt(plan, saved, tuned, mode);
      if (m !== mode) {
        mode = m;
        api.setMode(m);
      }
    }
    const b = bandAt(plan, tuned);
    if (b && b.id !== bandId && !(band && tuned / 1e6 >= band.low_mhz && tuned / 1e6 <= band.high_mhz)) bandId = b.id;
    recenter();
  }

  /** Tunes and slides the view so the station sits in the middle. */
  function tuneCentered(hz: number) {
    tuneTo(hz);
    slideView(tuned - span / 2, tuned + span / 2);
  }

  // an eased glide to a new view; dragging or zooming meanwhile stops it
  const SLIDE_MS = 260;
  let slideFrame = 0;
  function slideView(nvs: number, nve: number) {
    cancelAnimationFrame(slideFrame);
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return setView(nvs, nve);
    const [a, b] = [vs, ve];
    const t0 = performance.now();
    const frame = (now: number) => {
      const t = Math.min(1, (now - t0) / SLIDE_MS);
      const k = 1 - (1 - t) ** 3;
      setView(a + (nvs - a) * k, b + (nve - b) * k);
      if (t < 1) slideFrame = requestAnimationFrame(frame);
    };
    slideFrame = requestAnimationFrame(frame);
  }

  $effect(() => {
    if (!band || tuned < band.low_mhz * 1e6 || tuned > band.high_mhz * 1e6) return;
    lastHz[band.id] = tuned;
    try {
      localStorage.setItem('lastHz', JSON.stringify(lastHz));
    } catch {
      // remembering is a nicety
    }
  });

  let outside = $derived(!!band && (tuned < band.low_mhz * 1e6 || tuned > band.high_mhz * 1e6));

  function pickBand(b: Band) {
    bandId = b.id;
    panelPick = null;
    const s = Math.min(maxSpan, (b.high_mhz - b.low_mhz) * 1e6);
    const start = bandEntryHz(b, lastHz);
    let nvs = start - s / 2;
    nvs = Math.max(b.low_mhz * 1e6, Math.min(nvs, b.high_mhz * 1e6 - s));
    if (s >= (b.high_mhz - b.low_mhz) * 1e6) nvs = (b.low_mhz * 1e6 + b.high_mhz * 1e6) / 2 - s / 2;
    vs = nvs;
    ve = nvs + s;
    tuned = Math.round(start);
    mode = modeAt(plan, saved, tuned, b.mode);
    api.setMode(mode);
    recenter();
  }

  // ── signals ─────────────────────────────────────────

  let signals = $derived<(Signal & { steady: boolean })[]>(
    withSondes(
      withRds(
        (status?.signals ?? []).map((d) => label(plan, saved, d)),
        rdsHeard,
      ),
      status?.sondes ?? [],
    ).map((s) => ({ ...s, steady: persist.steady(s, Date.now(), UPDATES_PER_WINDOW) })),
  );
  let steadyCount = $derived(signals.filter((s) => s.steady && s.hz >= vs && s.hz <= ve).length);
  let listening = $derived(signals.find((s) => Math.abs(s.hz - tuned) <= Math.max(s.widthHz / 2, 3_000)));
  let listeningMark = $derived(saved.find((s) => Math.abs(s.mhz * 1e6 - tuned) < 1_500));

  // last frame's card frequencies, so cards don't follow every wobble of a measured centre
  let anchors: number[] = [];
  let cards = $derived.by(() => {
    const heard = signals.filter((s) => s.hz >= vs && s.hz <= ve && (showSteady || !s.steady));
    // on a channel plan every channel gets a card, so nobody has to remember frequencies
    const quiet = quietChannels(band?.id, vs, ve, heard).map((c) => ({
      hz: c.hz, widthHz: 0, snrDb: -1, title: c.name, sub: c.group ?? 'Nothing on it now', tag: 'QUIET', saved: false, channelHz: c.hz, quiet: true, steady: false,
    }));
    const inView = [...heard, ...quiet];
    const minSep = ((CARD_W + 8) / boxW) * span;
    const pxHz = span / boxW;
    const isTuned = (s: Signal) => s === listening || (!!s.quiet && Math.abs(s.hz - tuned) < 1_000);
    const placed = placeCards(inView, minSep, isTuned).map((p) => {
      const at =
        p.item === listening ? tuned : p.item.channelHz !== undefined ? p.item.channelHz : band?.mode === 'wfm' ? fmChannel(p.item.hz) : anchor(p.item.hz, anchors, Math.max(p.item.widthHz / 2, 10 * pxHz));
      const x = ((at - vs) / span) * boxW;
      return {
        ...p,
        at,
        x,
        left: Math.min(Math.max(x - CARD_W / 2, 6), boxW - CARD_W - 6),
        meter: Math.max(0, Math.min(100, Math.round((p.item.snrDb / 40) * 100))),
        selected: isTuned(p.item),
      };
    });
    anchors = placed.map((c) => c.at);
    return placed;
  });

  let dots = $derived.by(() => {
    const hidden = cards.flatMap((c) =>
      c.hidden.map((h) => ({ x: ((h.hz - vs) / span) * boxW, cls: h.quiet ? 'quiet' : 'hidden', title: `${fmtMHz(h.hz)} ${h.title} — ${h.quiet ? 'quiet; zoom in for its card' : 'behind a stronger signal; zoom in'}` })),
    );
    const quiet = saved
      .filter((s) => s.mhz * 1e6 >= vs && s.mhz * 1e6 <= ve && !signals.some((g) => Math.abs(g.hz - s.mhz * 1e6) < Math.max(g.widthHz / 2, 5_000)))
      .map((s) => ({ x: ((s.mhz * 1e6 - vs) / span) * boxW, cls: 'quiet', title: `${fmtMHz(s.mhz * 1e6)} ${s.name} — quiet` }));
    const steady = showSteady
      ? []
      : signals
          .filter((s) => s.steady && s.hz >= vs && s.hz <= ve)
          .map((s) => ({ x: ((s.hz - vs) / span) * boxW, cls: 'steady', title: `${fmtMHz(s.hz)} — steady carrier, probably interference` }));
    return [...hidden, ...quiet, ...steady];
  });

  function edge(side: 'left' | 'right') {
    const out = (hz: number) => (side === 'left' ? hz < vs : hz > ve);
    const heard = signals.filter((s) => out(s.hz) && (showSteady || !s.steady)).sort((a, b) => (side === 'left' ? b.hz - a.hz : a.hz - b.hz));
    if (heard.length) return { count: `${heard.length} on air`, nearest: heard[0].hz };
    const lo = band ? band.low_mhz * 1e6 : 0;
    const hi = band ? band.high_mhz * 1e6 : Infinity;
    const marks = saved
      .map((s) => s.mhz * 1e6)
      .filter((hz) => out(hz) && hz >= lo && hz <= hi)
      .sort((a, b) => (side === 'left' ? b - a : a - b));
    if (marks.length) return { count: `${marks.length} saved`, nearest: marks[0] };
    return null;
  }
  let stripMarks = $derived(band?.id === 'fm' ? stripTicks(heard.fm ?? [], saved.filter((x) => x.band.id === 'fm'), startedAt, band.low_mhz * 1e6, band.high_mhz * 1e6) : []);
  let leftEdge = $derived.by(() => (void signals, void vs, edge('left')));
  let rightEdge = $derived.by(() => (void signals, void ve, edge('right')));

  function step(dir: 1 | -1) {
    const heard = signals.filter((s) => showSteady || !s.steady).map((s) => s.hz);
    const lo = band ? band.low_mhz * 1e6 : -Infinity;
    const hi = band ? band.high_mhz * 1e6 : Infinity;
    const marks = saved.map((s) => s.mhz * 1e6).filter((hz) => hz >= lo && hz <= hi);
    const pick = (list: number[]) =>
      dir > 0 ? list.filter((f) => f > tuned + 1_000).sort((a, b) => a - b)[0] : list.filter((f) => f < tuned - 1_000).sort((a, b) => b - a)[0];
    const next = pick(heard) ?? pick(marks);
    if (next !== undefined) tuneTo(snapTune(next, status?.signals ?? [], saved, span / boxW, mode, band?.id));
  }

  // ── location ────────────────────────────────────
  // one setting for everything that depends on where you are: station passes, nearby repeaters

  let location = $state(readText('location'));
  function setLocation(v: string) {
    location = v.trim();
    try {
      localStorage.setItem('location', location);
    } catch {
      // without storage the location lasts until the app closes
    }
    loadIss();
    loadRepeaters();
  }

  let repeaters = $state<api.Repeater[]>([]);
  async function loadRepeaters() {
    repeaters = await api.repeaters(location);
    setRepeaters(repeaters, plan.bands);
    // channel lookups aren't reactive; a new plan object recomputes everything that uses them
    plan = { ...plan };
  }

  // ── space station ───────────────────────────────

  let iss = $state<Iss | null>(null);
  let nowS = $state(Date.now() / 1000);
  let issAge = $derived(ageDays(iss?.tle_epoch ?? null, nowS));
  let issUp = $derived(overhead(iss?.passes ?? [], nowS, issAge));
  let issNext = $derived(upcoming(iss?.passes ?? [], nowS).slice(0, 8));
  let issNote = $derived(freshnessNote(issAge));
  let railMarks = $derived<Record<string, { live: boolean; title: string; line?: string }>>(
    iss?.passes.length ? { iss: { live: issUp !== null, title: issTitle(iss.passes, nowS, issAge), line: issRailLine(iss.passes, nowS, issAge) } } : {},
  );

  function readText(key: string): string {
    try {
      return localStorage.getItem(key) ?? '';
    } catch {
      return '';
    }
  }
  // Ground track for the map; it covers 100 min ahead, so it is reloaded well before it runs out.
  let issTrack = $state<GroundPoint[]>([]);
  let issHere = $derived(groundAt(issTrack, nowS));
  // Zoom the map to the listener only while a pass is under way and its channel has signal.
  let issLocal = $derived(issUp !== null && (status?.channel_snr_db ?? 0) >= RECEPTION_DB);
  let issMapSummary = $derived(
    !issHere
      ? 'No orbit data yet'
      : `Over ${Math.abs(issHere.lat).toFixed(0)}° ${issHere.lat >= 0 ? 'N' : 'S'}, ${Math.abs(issHere.lon).toFixed(0)}° ${issHere.lon >= 0 ? 'E' : 'W'}` +
          (issLocal ? ' · zoomed to you while it is heard' : issUp ? ' · overhead, nothing heard yet' : ''),
  );

  async function loadIss() {
    issTrack = await api.issTrack();
    iss = await api.iss(location);
  }
  // the repeater panel: only on bands with a reach estimate, centred on the location
  let repReachKm = $derived(band ? REACH_KM[band.id] : undefined);
  let repInBand = $derived(band && repReachKm !== undefined ? repeaters.filter((r) => r.mhz >= band.low_mhz && r.mhz <= band.high_mhz) : []);
  let repPoints = $derived(band && iss?.site ? repeaterPoints(repInBand, band, iss.site) : []);
  let repTuned = $derived(repPoints.find((p) => Math.abs(p.hz - tuned) < 2_500) ?? null);
  let repNear = $derived(repPoints.filter((p) => p.reach));
  // strongest signal on each repeater's output right now
  let repLive = $derived.by(() => {
    const m = new Map<number, number>();
    for (const s of status?.signals ?? []) {
      const hz = channelAt(band?.id, s.hz)?.hz;
      if (hz !== undefined) m.set(hz, Math.max(m.get(hz) ?? -Infinity, s.snr_db));
    }
    return m;
  });
  // on-air repeaters first, so they show in the short list under the map; both groups stay nearest first, so a row moves
  // only when its repeater comes on or off the air. Sorting by signal strength reordered the rows on every status, and the
  // rows' flip animation measured each one, a full page layout per row
  let repRows = $derived(repNear.filter((p) => p !== repTuned).sort((a, b) => Number(repLive.has(b.hz)) - Number(repLive.has(a.hz))));
  let repSummary = $derived.by(() => {
    if (!iss?.site) return 'Set your location to see which are near';
    const maybe = repPoints.filter((p) => p.digital).length;
    return `${repNear.length} of ${repPoints.length} within ${repReachKm} km` + (maybe ? ` · dashed: ${maybe} may be digital only` : '');
  });

  // ── weather balloons ────────────────────────────

  let decoding = $derived(band?.decoder === 'radiosonde');
  let sondes = $derived(status?.sondes ?? []);
  let sondeLatest = $derived(sondes.reduce<(typeof sondes)[number] | null>((a, o) => (a && a.age_s <= o.age_s ? a : o), null));
  let sondeLine = $derived(sondeLatest ? `${sondes.length} heard · latest ${sondeName(sondeLatest)}, ${sondeSummary(sondeLatest)}` : 'None decoded yet');
  $effect(() => {
    api.setSondes(decoding);
  });

  // weather services launch twice a day at about 00:00 and 12:00 UTC
  const launchTimes = [0, 12]
    .map((h) => {
      const d = new Date();
      d.setUTCHours(h, 0, 0, 0);
      return d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
    })
    .join(' and ');

  function ago(secs: number): string {
    if (secs < 5) return 'just now';
    if (secs < 90) return `${Math.round(secs)} s ago`;
    return `${Math.round(secs / 60)} min ago`;
  }

  // ── header text ─────────────────────────────────────

  let rds = $derived(status?.rds && status.mode === 'wfm' ? status.rds : null);

  // ── HD Radio ────────────────────────────────────────

  let hdProgram = $derived(mode === 'wfm' ? (hdPrefs[fmChannel(tuned)] ?? null) : null);
  $effect(() => {
    api.setHd(hdProgram);
  });
  let hd = $derived(status?.hd && status.mode === 'wfm' ? status.hd : null);
  let hdImages = $state<api.HdImage[]>([]);
  let hdPicture = $derived(hdImages.find((i) => i.kind === 'art') ?? hdImages.find((i) => i.kind === 'logo'));
  let hdTraffic = $derived(trafficGrid(hdImages));
  let hdWeather = $derived(hdImages.find((i) => i.kind === 'weather'));
  let p25 = $derived(status?.mode === 'p25' ? (status.p25 ?? null) : null);
  // digital audio drops out for a moment on a weak signal; hold the speaker lit through that
  let hdAudioAt = $state(0);
  let hdAudio = $derived(!!hd && (hd.digital || nowS * 1000 - hdAudioAt < HD_AUDIO_HOLD_MS));
  let hdSummary = $derived(hd ? [hd.name, hd.title && [hd.title, hd.artist].filter(Boolean).join(' — '), hd.error].filter(Boolean).join(' · ') : '');
  function setHdProgram(p: number | null) {
    const key = String(fmChannel(tuned));
    const next = { ...hdPrefs };
    if (p === null) delete next[key];
    else next[key] = p;
    hdPrefs = next;
    try {
      localStorage.setItem('hd', JSON.stringify(next));
    } catch {
      // not remembered across restarts
    }
  }
  function toggleHd(e: Event) {
    e.stopPropagation();
    setHdProgram(nextHd(hdProgram, hd?.programs ?? []));
  }
  let tunedChannel = $derived(channelAt(band?.id, tuned));
  let nowTitle = $derived((rds && rdsName(rds)) ?? listeningMark?.name ?? (listening?.byFreq ? null : listening?.title) ?? tunedChannel?.name ?? (band ? band.name : 'Tuned'));
  let muted = $derived(playing && status?.squelch_open === false);
  let nowSub = $derived(
    muted ? 'Muted — signal is below the squelch level' :
    hd?.digital && hd.title ? [hd.title, hd.artist].filter(Boolean).join(' — ') :
    rds && rdsNow(rds) ? rdsNow(rds)! : listening ? listening.sub : listeningMark ? 'Quiet right now' : 'Between signals — press → for the next one',
  );
  let meter = $derived(playing ? Math.max(0, Math.min(100, ((status?.channel_snr_db ?? 0) / METER_DB) * 100)) : 0);

  // ── theme ───────────────────────────────────────────

  let hues = $derived(Object.fromEntries(plan.groups.map((g) => [g.id, groupHue(theme, g.id, plan.groups.map((x) => x.id))])));
  let hue = $derived((band && hues[band.group]) ?? theme.accent);
  let stops = $derived(waterfallStops(theme));
  // every strength bar in the page wears the waterfall's ramp on the meter's dB scale
  let ramp = $derived(rampGradient(stops, METER_DB, setup.below_floor_db, setup.range_db));
  // the local map's shaded terrain, in theme colours
  let mapPalette = $derived({ plain: theme.lighter_background, peak: mix(theme.muted, theme.light_foreground, 0.15), shadow: theme.darker_background, light: theme.light_foreground });
  let wfColors = $derived({
    bg: theme.darker_background,
    grid: theme.lighter_background,
    line: theme.foreground,
    fill: hue + '2e',
    accent: hue,
    tick: theme.light_foreground,
  });

  // ── panels ──────────────────────────────────────────
  // One panel beside the waterfall: the most relevant one. Others that matter right now are tabs in its title.

  type PanelId = 'sondes' | 'hd' | 'p25' | 'iss' | 'repeaters' | 'channels';
  let panelPick = $state<PanelId | null>(null);
  let panels = $derived.by(() => {
    const own: PanelId[] = [];
    if (band?.id === 'iss') own.push('iss');
    if (decoding) own.push('sondes');
    if (p25) own.push('p25');
    if (repInBand.length && repReachKm !== undefined) own.push('repeaters');
    // decoded in the background as soon as the station shows HD sidebands
    if (hd) own.push('hd');
    if (chList.length) own.push('channels');
    // a pass overhead is worth a tab on any band, but never opens a panel by itself
    if (own.length && issUp && !own.includes('iss')) own.push('iss');
    return own;
  });
  let chList = $derived(channelList(band?.id, status?.signals ?? []));
  let chLive = $derived(chList.reduce((n, s) => n + s.channels.filter((c) => c.live).length, 0));
  let chSummary = $derived(chLive ? `${chLive} on air` : 'Nothing on air right now');
  let panel = $derived(panelPick && panels.includes(panelPick) ? panelPick : (panels[0] ?? null));
  const PANEL_TITLE: Record<PanelId, string> = { sondes: 'Weather balloons', hd: 'HD Radio', p25: 'Police and fire', iss: 'Space station', repeaters: 'Repeaters', channels: 'Channels' };
  let issHue = $derived(hues[plan.bands.find((b) => b.id === 'iss')?.group ?? ''] ?? hue);
  const panelHue = (id: PanelId) => (id === 'iss' ? issHue : hue);
  let panelSummary = $derived(
    panel === 'sondes' ? sondeLine : panel === 'hd' ? hdSummary : panel === 'p25' && p25 ? p25Summary(p25) : panel === 'iss' ? issMapSummary : panel === 'repeaters' ? repSummary : panel === 'channels' ? chSummary : '',
  );

  let wfSummary = $derived(
    `${band?.name ?? 'Tuned'} · ${fmtMHz(vs)} – ${fmtMHz(ve)} MHz` +
      (decoding ? ` · decoding weather balloons on ${status?.decoding_hz.length ?? 0} signal${status?.decoding_hz.length === 1 ? '' : 's'}` : band?.decoder ? ` · the ${band.decoder} decoder is not built yet; this is the raw signal` : ''),
  );

  // which pane has the screen; it lets go when the panel changes
  let maxed = $state<{ pane: 'wf' | 'panel'; panel: PanelId } | null>(null);
  let maxPane = $derived(maxed && maxed.panel === panel ? maxed.pane : null);
  function toggleMax(pane: 'wf' | 'panel') {
    if (!panel) return;
    maxed = maxPane === pane ? null : { pane, panel };
  }

  // the panel's share of the width (or height when stacked), dragged at the divider
  let panelShare = $state(readNumber('panel.size', 30));
  let stacked = $state(false);
  let mainEl: HTMLElement;
  function startDivider(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const grip = e.currentTarget as HTMLElement;
    grip.setPointerCapture(e.pointerId);
    const r = mainEl.getBoundingClientRect();
    const move = (m: PointerEvent) => {
      const v = stacked ? ((r.bottom - m.clientY) / r.height) * 100 : ((r.right - m.clientX) / r.width) * 100;
      panelShare = Math.round(Math.min(75, Math.max(20, v)));
    };
    const stop = () => {
      grip.removeEventListener('pointermove', move);
      grip.removeEventListener('pointerup', stop);
      remember('panel.size', String(panelShare));
    };
    grip.addEventListener('pointermove', move);
    grip.addEventListener('pointerup', stop);
  }

  // drag, zoom and tune are shown once, until dismissed
  let hintSeen = $state(readNumber('hint.seen', 0) === 1);
  function dismissHint() {
    hintSeen = true;
    remember('hint.seen', '1');
  }

  // ── squelch on the signal meter ─────────────────────
  // the mark is the squelch level, and far left turns it off

  function dragSquelch(e: PointerEvent) {
    if (e.button !== 0) return;
    const track = e.currentTarget as HTMLElement;
    track.setPointerCapture(e.pointerId);
    const r = track.getBoundingClientRect();
    const set = (m: PointerEvent) => {
      const db = Math.round(((m.clientX - r.left) / r.width) * METER_DB);
      squelch = db < 1 ? 0 : Math.min(METER_DB, db);
    };
    set(e);
    const stop = () => {
      track.removeEventListener('pointermove', set);
      track.removeEventListener('pointerup', stop);
    };
    track.addEventListener('pointermove', set);
    track.addEventListener('pointerup', stop);
  }
  function squelchKey(e: KeyboardEvent) {
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') squelch = Math.min(METER_DB, squelch + 1);
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') squelch = Math.max(0, squelch - 1);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  function setTheme(t: Theme) {
    theme = resolve(t);
    applyTheme(theme);
  }

  // ── window ──────────────────────────────────────────

  async function toggleFull() {
    full = !full;
    try {
      await getCurrentWindow().setFullscreen(full);
    } catch {
      // fullscreen is a nicety; the layout change alone still works
    }
  }

  function togglePlay() {
    playing = !playing;
    api.setPlaying(playing);
  }

  async function toggleRecord() {
    await api.record(!rec?.recording);
    // the engine starts or ends the file on its next pass
    setTimeout(async () => {
      rec = await api.recording();
      recAt = Date.now() / 1000 - rec.secs;
    }, 250);
  }

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
    const keyed = railPlan.groups.flatMap((g) => railPlan.bands.filter((b) => b.group === g.id)).slice(0, 9);
    if (/^[1-9]$/.test(e.key) && keyed[+e.key - 1]) pickBand(keyed[+e.key - 1]);
    else if (e.key === 'ArrowRight') step(1);
    else if (e.key === 'ArrowLeft') step(-1);
    else if (e.key === '[') setView(vs - span / 4, ve - span / 4);
    else if (e.key === ']') setView(vs + span / 4, ve + span / 4);
    else if (e.key === '-') zoom(1.5);
    else if (e.key === '=' || e.key === '+') zoom(1 / 1.5);
    else if (e.key === 'f') toggleMax('panel');
    else if (e.key === 'F11') toggleFull();
    else if (e.key === 'Escape' && settingsOpen) settingsOpen = false;
    else if (e.key === 'Escape' && maxPane) maxed = null;
    else if (e.key === ' ') togglePlay();
    else if (e.key === 's') settingsOpen = !settingsOpen;
    else return;
    e.preventDefault();
  }

  function zoom(k: number) {
    const c = (vs + ve) / 2;
    setView(c - (span * k) / 2, c + (span * k) / 2);
  }

  onMount(() => {
    const unsubs: Promise<() => void>[] = [];
    (async () => {
      setup = await api.setup();
      setTheme(await api.theme());
      const b = await api.bands();
      repeaters = await api.repeaters(location);
      setRepeaters(repeaters, b.plan.bands);
      plan = b.plan;
      bandsPath = b.path;
      bandsError = b.error;
      await api.start(
        (row) => waterfall?.push(row),
        (s) => {
          if (s.rds && s.mode === 'wfm') {
            const ch = fmChannel(s.tuned_hz);
            const old = rdsHeard.get(ch);
            if (JSON.stringify(old) !== JSON.stringify(s.rds)) rdsHeard = new Map(rdsHeard).set(ch, s.rds);
          }
          if (s.mode === 'wfm' && band?.id === 'fm') {
            const list = heard.fm ?? [];
            const next = noteHeard(list, s.signals, rdsHeard, Date.now());
            if (next !== list) {
              heard = { ...heard, fm: next };
              remember('stations', JSON.stringify(heard));
            }
          }
          persist.update(s.signals.map((d) => d.hz), Date.now());
          if (s.hd_images) hdImages = s.hd_images;
          if (!s.hd) hdImages = [];
          if (s.hd?.digital) hdAudioAt = Date.now();
          status = s;
        },
      );
      const fm = plan.bands.find((x) => x.id === 'fm') ?? plan.bands[0];
      if (fm) pickBand(fm);
      loadIss();
      unsubs.push(api.onIss(loadIss));
      unsubs.push(api.onTheme(setTheme));
      unsubs.push(
        api.onBands((nb) => {
          setRepeaters(repeaters, nb.plan.bands);
          plan = nb.plan;
          bandsPath = nb.path;
          bandsError = nb.error;
        }),
      );
      unsubs.push(
        api.onRepeaters(loadRepeaters),
      );
    })();
    // Drives the header clocks and keeps the pass list current.
    const tick = setInterval(() => (nowS = Date.now() / 1000), 1000);
    const reload = setInterval(loadIss, 5 * 60_000);
    return () => {
      clearInterval(tick);
      clearInterval(reload);
      unsubs.forEach((u) => u.then((f) => f()));
    };
  });

  $effect(() => {
    api.setVolume(volume);
  });
  $effect(() => {
    api.setSquelch(squelch > 0 ? squelch : null);
    try {
      localStorage.setItem('squelch', String(squelch));
    } catch {
      // remembering the level is a nicety
    }
  });
  $effect(() => {
    api.setGain(gain === 'auto' ? null : +gain);
  });
  $effect(() => {
    api.setPpm(ppm);
    remember('ppm', String(ppm));
  });
  $effect(() => {
    const m = status?.crystal ? Math.round(status.crystal.ppm) : null;
    if (!ppmByHand && m !== null && Math.abs(m) <= MAX_PPM && m !== ppm) ppm = m;
  });
  function setPpmByHand(v: string) {
    ppmByHand = v.trim() !== '' && Number.isFinite(+v);
    remember('ppm.hand', ppmByHand ? '1' : '0');
    if (ppmByHand) ppm = Math.max(-MAX_PPM, Math.min(MAX_PPM, Math.round(+v)));
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app" style:--accent={hue} style:--hue={hue} style:--ramp={ramp}>
  {#if !full}
    <BandRail plan={railPlan} current={bandId} {outside} open={railOpen} onPick={pickBand} onToggle={toggleRail} {hues} marks={railMarks} {settingsOpen} onSettings={() => (settingsOpen = !settingsOpen)} {settings} />
  {/if}

  <div class="right">
    <header>
      <div class="freq">
        <span class="digits mono"><em>{fmtHz(tuned).split('.')[0]}</em>.{fmtHz(tuned).split('.').slice(1).join('.')}</span>
        <span class="unit mono">MHz</span>
        {#if outside}<span class="outside mono">outside {band?.name}</span>{/if}
      </div>
      <div class="now">
        <span class="title">{nowTitle}</span>
        <span class="sub copy">{nowSub}</span>
      </div>
      <button class="play" class:on={playing} onclick={togglePlay} aria-label={playing ? 'Pause' : 'Play'} title="{playing ? 'Pause' : 'Play'} (space)">
        <Icon name={playing ? 'pause' : 'play'} size={20} />
      </button>
      <button class="rec" class:on={rec?.recording} onclick={toggleRecord} aria-label={rec?.recording ? 'Stop recording' : 'Record'} title={rec?.recording ? 'Stop recording' : 'Record what you hear to a file'}>
        <Icon name="record" size={18} />
      </button>
      {#if rec?.recording}
        <span class="rec-note mono">{Math.floor(Math.max(0, nowS - recAt) / 60)}:{String(Math.floor(Math.max(0, nowS - recAt)) % 60).padStart(2, '0')}</span>
      {:else if rec?.error}
        <span class="rec-note err copy" title={rec.error}>Not recording: {rec.error}</span>
      {:else if rec?.file}
        <span class="rec-note copy" title={rec.file}>Saved {rec.file.split('/').pop()}</span>
      {/if}
      <div class="levels">
        <div class="ctl">
          <span>Signal · squelch {squelch > 0 ? `${squelch} dB` : 'off'}</span>
          <div
            class="track"
            role="slider"
            tabindex="0"
            aria-label="Squelch"
            aria-valuemin={0}
            aria-valuemax={METER_DB}
            aria-valuenow={squelch}
            aria-valuetext={squelch > 0 ? `${squelch} dB` : 'Off'}
            title="Squelch: drag the mark. Audio stays muted while the signal is left of it. Far left turns it off."
            onpointerdown={dragSquelch}
            onkeydown={squelchKey}
          >
            <div class="fillclip"><div class="fill" class:muted style:transform="translateX({meter - 100}%)"></div></div>
            <span class="sq-mark" class:off={squelch === 0} style:left="{(squelch / METER_DB) * 100}%"></span>
          </div>
        </div>
        <label class="ctl">
          <span>Volume</span>
          <input type="range" min="0" max="1.5" step="0.05" bind:value={volume} />
        </label>
      </div>
      <div class="clocks mono" title="Local time and UTC">
        <span>{localTime(nowS)}</span>
        <span>{utcTime(nowS)} <small>UTC</small></span>
      </div>
      {#if status?.simulated}<span class="chip warn" title={status.device}>Simulated</span>{/if}
      {#if status?.error || bandsError}<button class="chip err" onclick={() => (settingsOpen = true)} title={status?.error ?? bandsError ?? ''}>Problem · details</button>{/if}
      <button class="iconbtn" onclick={toggleFull} title={full ? 'Exit full screen (F11)' : 'Full screen (F11)'} aria-label={full ? 'Exit full screen' : 'Full screen'}>
        <Icon name={full ? 'shrink' : 'expand'} size={18} />
      </button>
    </header>

    <main bind:this={mainEl} bind:clientWidth={null, (w) => (stacked = (w ?? 0) < 1000)} class:stacked class:has-panel={panel} class:max-wf={maxPane === 'wf'} class:max-panel={maxPane === 'panel'} style:--share="{panelShare}%">
      <Pane title="Spectrum" summary={wfSummary} maxed={maxPane === 'wf'} onMax={panel ? () => toggleMax('wf') : undefined}>
        {#snippet tools()}
          {#if steadyCount || showSteady}
            <button class:on={!showSteady} onclick={() => (showSteady = !showSteady)} title="Narrow carriers that never switch off are usually interference from nearby electronics">
              {showSteady ? 'Hide steady carriers' : `${steadyCount} steady hidden`}
            </button>
          {/if}
          <button onclick={() => zoom(1.5)} aria-label="Zoom out" title="Zoom out (-)">−</button>
          <button onclick={() => zoom(1 / 1.5)} aria-label="Zoom in" title="Zoom in (+)">+</button>
          <button onclick={() => setView((vs + ve) / 2 - maxSpan / 2, (vs + ve) / 2 + maxSpan / 2)} title="Show as much of the band as fits">Widest</button>
        {/snippet}
        <div class="box" class:panning class:strip={maxPane === 'panel'} bind:clientWidth={boxW}>
          {#if maxPane !== 'panel'}
            <div class="lane" style:height="{LANE_H}px">
              {#each dots as d}
                <span class="dot {d.cls}" style:left="{d.x}px" title={d.title}></span>
              {/each}
              {#each cards as c (c.at)}
                <span class="stem" class:sel={c.selected} style:left="{c.x}px"></span>
                <button class="card" class:sel={c.selected} class:hdon={c.selected && hdProgram !== null} class:quiet={c.item.quiet} style:left="{c.left}px" style:width="{CARD_W}px" onclick={() => tuneCentered(snapTune(c.item.hz, status?.signals ?? [], saved, span / boxW, mode, band?.id))}>
                  <span class="row1">
                    <span class="ttl">{c.item.title}</span>
                    {#if c.hidden.length}
                      <span class="more mono" title={c.hidden.map((h) => `${fmtMHz(h.hz)} ${h.title}`).join(', ') + ' — zoom in to separate'}>+{c.hidden.length}</span>
                    {/if}
                    {#if c.selected && mode === 'wfm' && (hd || status?.hd_sidebands || hdProgram !== null)}
                      <span class="hdchip" class:on={hdProgram !== null} role="switch" aria-checked={hdProgram !== null} tabindex="0" title="This station also broadcasts digitally (HD Radio). Click to cycle through its programs and back to analog." onclick={toggleHd} onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && toggleHd(e)}>{hdProgram === null ? 'HD' : hdLabel(hdProgram)}</span>
                    {/if}
                  </span>
                  <span class="row2">{c.item.sub}</span>
                  <span class="row3 mono">
                    <!-- an FM card's title already says where it is on the dial -->
                    {#if mode !== 'wfm'}<span>{fmtMHz(c.at)}</span>{/if}
                    {#if c.item.tag !== 'HEARD'}<span class="tag" class:saved={c.item.saved}>{c.item.tag}</span>{/if}
                    <span class="bar"><Level pct={c.meter} width="100%" height="3px" /></span>
                  </span>
                </button>
              {/each}
            </div>
          {/if}
          <div class="wf">
            <Waterfall
              bind:this={waterfall}
              {vs}
              {ve}
              {rate}
              {stops}
              {tuned}
              bandwidth={BANDWIDTH[mode]}
              specH={maxPane === 'panel' ? STRIP_SPEC_H : SPEC_H}
              colors={wfColors}
              onTune={(hz, pxHz) => tuneTo(snapTune(hz, status?.signals ?? [], saved, pxHz, mode, band?.id))}
              onView={(a, b) => {
                cancelAnimationFrame(slideFrame);
                setView(a, b);
              }}
              bandId={band?.id}
              band={band ? { lo: band.low_mhz * 1e6, hi: band.high_mhz * 1e6, name: band.name } : undefined}
            />
          </div>
          {#if band?.id === 'fm' && maxPane !== 'panel'}
            <BandStrip
              lo={band.low_mhz * 1e6}
              hi={band.high_mhz * 1e6}
              {vs}
              {ve}
              {tuned}
              ticks={stripMarks}
              {hue}
              onTune={tuneCentered}
              onView={(a, b) => {
                cancelAnimationFrame(slideFrame);
                setView(a, b);
              }}
              onGlide={slideView}
            />
          {/if}
          {#if maxPane !== 'panel'}
            {#if leftEdge}
              {@const e = leftEdge}
              <button class="edge left" style:top="{LANE_H + SPEC_H + 40}px" onclick={() => tuneTo(e.nearest)} title="Jump to the nearest signal on the left">
                <span class="arrow">‹</span>
                <span class="txt"><b>{e.count}</b><span class="mono">nearest {fmtMHz(e.nearest)}</span></span>
              </button>
            {/if}
            {#if rightEdge}
              {@const e = rightEdge}
              <button class="edge right" style:top="{LANE_H + SPEC_H + 40}px" onclick={() => tuneTo(e.nearest)} title="Jump to the nearest signal on the right">
                <span class="txt"><b>{e.count}</b><span class="mono">nearest {fmtMHz(e.nearest)}</span></span>
                <span class="arrow">›</span>
              </button>
            {/if}
            {#if !hintSeen}
              <div class="hint">
                <span>Drag sideways to move · scroll to zoom · click a signal to tune</span>
                <button onclick={dismissHint}>Got it</button>
              </div>
            {/if}
          {/if}
        </div>
      </Pane>

      {#if panel}
        <button class="divider" aria-label="Resize the panel" title="Drag to give the panel or the waterfall more room" onpointerdown={startDivider}></button>
        <Pane title={PANEL_TITLE[panel]} summary={panelSummary} accent={panelHue(panel)} maxed={maxPane === 'panel'} mini={maxPane === 'wf'} onMax={() => toggleMax(maxPane === 'wf' ? 'wf' : 'panel')}>
          {#snippet tabs()}
            {#each panels.filter((p) => p !== panel) as p (p)}
              <button class="ptab" style:--c={panelHue(p)} onclick={() => (panelPick = p)} title="Also relevant now: show {PANEL_TITLE[p]}">
                {PANEL_TITLE[p]}{#if p === 'iss' && issUp}<b> • now</b>{/if}
              </button>
            {/each}
          {/snippet}
          {#if panel === 'sondes'}
            <div class="pbody wrap">
              {#each sondes as o (o.hz)}
                {@const drift = sondeDrift(o)}
                <button class="item balloon" class:sel={Math.abs(o.hz - tuned) < 10_000} onclick={() => tuneTo(o.hz)}>
                  <span class="name">{sondeName(o)}</span>
                  <span class="big">{sondeSummary(o)}</span>
                  {#if drift}<span class="dim">{drift}</span>{/if}
                  {#if o.lat !== null && o.lon !== null}<span class="mono dim">{o.lat.toFixed(4)}, {o.lon.toFixed(4)}</span>{/if}
                  <span class="mono dim">{fmtMHz(o.hz)} MHz{o.time ? ` · ${o.time} UTC` : ''} · {ago(o.age_s)}</span>
                </button>
              {:else}
                <p class="empty">None decoded yet. Each narrow signal in view gets about 20 seconds of listening.</p>
                <p class="empty">Weather services launch around {launchTimes} your time. A balloon flies for about two hours and can be heard a few hundred kilometres away; an antenna outdoors helps.</p>
              {/each}
            </div>
          {:else if panel === 'hd' && hd}
            <div class="pbody hdbody">
              <div class="col">
                <div class="item">
                  {#if hdPicture}
                    <img class="hdpic" src={hdPicture.data_url} alt={hdPicture.kind === 'art' ? 'Album art' : 'Station logo'} />
                  {:else if hd.incoming !== null}
                    <div class="hdwait" title="A picture is arriving"><Icon name="image" size={36} /></div>
                  {/if}
                  {#if hd.incoming !== null}<div class="hdload" title="Picture {Math.round(hd.incoming * 100)}% here"><span style:width="{hd.incoming * 100}%"></span></div>{/if}
                  {#if hd.name}<span class="name">{hd.name}</span>{/if}
                  {#if hd.slogan}<span class="dim">{hd.slogan}</span>{/if}
                  {#if hd.title}<span class="big">{hd.title}</span>{/if}
                  {#if hd.artist || hd.album}<span>{[hd.artist, hd.album].filter(Boolean).join(' · ')}</span>{/if}
                  {#if hd.error}
                    <span class="dim">{hd.error}</span>
                  {:else}
                    <span class="hdstat" title={hdState({ ...hd, digital: hdAudio }, hdProgram !== null)}>
                      <span class="step" class:on={hd.synced} class:seek={!hd.synced}><Icon name="tower" size={20} /></span>
                      <span class="step" class:on={hdAudio} class:live={hdAudio && hdProgram !== null}><Icon name="speaker" size={20} /></span>
                      <Level pct={hd.synced && hd.mer_db !== null ? ((hd.mer_db - HD_MER_DB[0]) / (HD_MER_DB[1] - HD_MER_DB[0])) * 100 : 0} width="110px" height="10px" />
                    </span>
                  {/if}
                  {#if hd.message}<span class="dim">{hd.message}</span>{/if}
                </div>
                {#if hd.synced}
                  <div class="hdprogs">
                    <button class:sel={hdProgram === null} onclick={() => setHdProgram(null)} title="Analog FM">FM</button>
                    {#each hd.programs.length ? hd.programs : [0] as p}
                      <button class:sel={p === hdProgram} onclick={() => setHdProgram(p)} title="Digital program {p + 1}">{hdLabel(p)}</button>
                    {/each}
                  </div>
                {/if}
              </div>
              {#if hdTraffic}
                <div class="col">
                  <h2>Traffic</h2>
                  <div class="hdgrid">
                    {#each hdTraffic as t}
                      {#if t}<img src={t} alt="" />{:else}<span></span>{/if}
                    {/each}
                  </div>
                </div>
              {/if}
              {#if hdWeather}
                <div class="col">
                  <h2>Weather radar</h2>
                  <img class="hdpic" src={hdWeather.data_url} alt="Weather radar" />
                </div>
              {/if}
            </div>
          {:else if panel === 'p25' && p25}
            <div class="pbody wrap">
              <div class="col">
                <div class="item">
                  <span class="name">{p25Site(p25) ?? (p25.state === 'searching' ? 'Looking for a control channel near this frequency…' : 'Reading the site…')}</span>
                  {#if p25.control_hz !== null}<span class="mono dim">control {fmtMHz(p25.control_hz)} MHz · {p25.messages_per_sec.toFixed(0)} messages/s{p25.ppm !== null ? ` · dongle ${p25.ppm.toFixed(0)} ppm` : ''}</span>{/if}
                  {#if p25.voice_error}<span class="dim">{p25.voice_error}</span>{/if}
                  <span class="dim">Plays unencrypted Phase 1 calls as the site assigns them, one at a time.</span>
                </div>
              </div>
              <div class="col">
                <h2>Calls</h2>
                {#each p25.recent as c (c.at_secs + ':' + c.hz)}
                  {@const block = callBlock(c, p25.voice_error)}
                  {@const live = p25.call?.group === c.group && p25.call?.hz === c.hz}
                  <div class="item" class:sel={live}>
                    <span class="name">Talkgroup {c.group}{c.emergency ? ' · EMERGENCY' : ''}</span>
                    <span class="dim">{live ? `Playing${p25.voice_errors_pct > 20 ? ` · ${p25.voice_errors_pct.toFixed(0)}% damaged` : ''}` : (block ?? 'Ended')}</span>
                    <span class="mono dim">{fmtMHz(c.hz)} MHz{c.source ? ` · radio ${c.source}` : ''} · {callAge(c, p25.now_secs)}</span>
                  </div>
                {:else}
                  <p class="empty">No calls yet. Busy sites assign several a minute; quiet ones can go minutes between.</p>
                {/each}
              </div>
            </div>
          {:else if panel === 'iss'}
            <div class="split over">
              <div class="mapbox issmap">
                <IssMap track={issTrack} now={nowS} site={iss?.site ?? null} local={issLocal} />
                <span class="maplabel" class:warn={freshness(issAge) === 'aging'} class:err={freshness(issAge) === 'stale' || freshness(issAge) === 'none'}>{ageLabel(issAge)}</span>
              </div>
              <div class="list">
                {#if iss?.bad_location}<p class="empty err">Your location in Settings is not a US ZIP code, state or “lat, lon”.</p>{/if}
                {#if iss?.approximate}<p class="empty">Times are for the middle of the state: within a minute nearby, a few minutes off in a far corner of a big state. A ZIP code is closer.</p>{/if}
                <p class="empty" class:warn={freshness(issAge) === 'aging'} class:err={freshness(issAge) === 'stale'}>{issNote || `${ageLabel(issAge)}.`}</p>
                {#if !location}
                  <p class="empty">Set your location in Settings (S) to see when the station passes over.</p>
                {:else}
                  {#each issNext as p (p.rise)}
                    {@const now = p === issUp}
                    <div class="item" class:sel={now}>
                      <span class="name">{now ? 'Overhead now' : `${dayLabel(p.rise, nowS)} ${clock(p.rise)}`}</span>
                      <span class="big">{Math.round(p.max_elevation)}° high</span>
                      <span class="dim">{now ? `sets ${until(p.set, nowS)}` : until(p.rise, nowS)} · {Math.round((p.set - p.rise) / 60)} min above the horizon</span>
                    </div>
                  {/each}
                  <p class="empty">Passes below about 10° are hard to hear. Voice and pictures are on 145.800 MHz, APRS on 145.825 MHz; the station transmits only some of the time.</p>
                {/if}
              </div>
            </div>
          {:else if panel === 'channels'}
            <div class="list chlist">
              {#each chList as sec, i (i)}
                {#if sec.group}<h3 class="chgroup">{sec.group}</h3>{/if}
                {#each sec.channels as { channel: c, live, snr } (c.hz)}
                  <button class="reprow chrow" class:sel={c.hz === tunedChannel?.hz} class:live onclick={() => tuneTo(c.hz)} title={live ? `On air${snr !== null ? `, ${snr.toFixed(0)} dB above the noise` : ''}` : undefined}>
                    <span class="dot"></span>
                    <span class="name town">{c.name}</span>
                    {#if snr !== null}<Level pct={(snr / METER_DB) * 100} />{/if}
                    <span class="mono">{(c.hz / 1e6).toFixed(4)}</span>
                  </button>
                {/each}
              {/each}
            </div>
          {:else if panel === 'repeaters'}
            {#if iss?.site}
              <div class="split">
                <div class="mapbox repmap">
                  <RepeaterMap points={repPoints} reachKm={repReachKm ?? 0} tunedHz={repTuned?.hz ?? null} live={new Set(repLive.keys())} onTune={(hz) => tuneTo(hz)} site={iss.site} palette={mapPalette} />
                  <span class="maplabel">{repTuned ? `Ring: ${repTuned.call}'s reach, about ${repReachKm} km; a guess, a mountaintop repeater reaches further` : `Filled: probably in reach, within about ${repReachKm} km`}</span>
                </div>
                <div class="list">
                  {#if repTuned}
                    <div class="item sel">
                      <span class="name">{repTuned.call}</span>
                      <span class="dim">{channelAt(band?.id, repTuned.hz)?.group ?? repTuned.city} · {Math.round(repTuned.km)} km</span>
                      {#if repTuned.digital}<span class="warn">Listed as {repTuned.digital} too. If it is digital only, it will sound like buzzing.</span>{/if}
                    </div>
                  {/if}
                  {#each repRows as p (p.hz)}
                    {@const snr = repLive.get(p.hz)}
                    <button class="reprow chrow" class:live={snr !== undefined} onclick={() => tuneTo(p.hz)} title={snr !== undefined ? `On air, ${snr.toFixed(0)} dB above the noise` : undefined}>
                      <span class="dot"></span>
                      <span class="name">{p.call}</span>
                      <span class="dim town">{p.city}</span>
                      {#if p.digital}<span class="warn mono">{p.digital}?</span>{/if}
                      {#if snr !== undefined}<Level pct={(snr / METER_DB) * 100} />{/if}
                      <span class="mono">{(p.hz / 1e6).toFixed(4)}</span>
                      <span class="mono dim">{Math.round(p.km)} km</span>
                    </button>
                  {:else}
                    <p class="empty">None within {repReachKm} km of your location.</p>
                  {/each}
                </div>
              </div>
            {:else}
              <p class="empty pbody">Set your location in Settings (S) to see which repeaters are near you.</p>
            {/if}
          {/if}
        </Pane>
      {/if}
    </main>
  </div>
</div>

{#snippet settings()}
  <label class="field">
    <span class="lbl">Location</span>
    <input class="loc" value={location} placeholder="80302, Colorado, or 39.74, -104.99" class:bad={iss?.bad_location} onchange={(e) => setLocation(e.currentTarget.value)} />
    <small class:err={iss?.bad_location}>{iss?.bad_location ? 'Not a US ZIP code, state or “lat, lon”.' : 'ZIP code, state, or lat, lon. Times space station passes and picks nearby repeaters.'}</small>
  </label>
  <label class="field">
    <span class="lbl">Gain</span>
    <select bind:value={gain}>
      <option value="auto">Auto</option>
      {#each status?.gains ?? [] as g}
        <option value={String(g)}>{(g / 10).toFixed(1)} dB</option>
      {/each}
    </select>
  </label>
  <label class="field">
    <span class="lbl">Crystal correction, ppm</span>
    <input class="loc" type="number" value={ppmByHand ? ppm : ''} placeholder={`${ppm} (measured)`} onchange={(e) => setPpmByHand(e.currentTarget.value)} />
    <small>{ppmByHand
      ? 'Set by hand. Clear it to measure again.'
      : status?.crystal
        ? `Measured on ${fmtMHz(status.crystal.hz)} MHz: the dongle runs ${Math.abs(status.crystal.ppm).toFixed(0)} ppm ${status.crystal.ppm > 0 ? 'fast' : 'slow'}.`
        : ppm
          ? 'Measured earlier. A few seconds on a strong FM station measures it again.'
          : 'Not measured yet. A few seconds on a strong FM station measures it.'}</small>
  </label>
  <label class="field toggle">
    <span>Show quiet bands<small>Data and tracking, railroad and military aircraft: bands that have given us nothing so far</small></span>
    <input type="checkbox" checked={showDeadAir} onchange={toggleDeadAir} />
  </label>
  {#if status?.error || bandsError}
    <div class="field">
      <span class="lbl">Problems</span>
      {#if status?.error}<small class="err copy">{status.error}</small>{/if}
      {#if bandsError}<small class="err copy">Bands file: {bandsError}</small>{/if}
    </div>
  {/if}
  <div class="field">
    <span class="lbl">Receiver</span>
    <small class:warn={status?.simulated}>{status?.device ?? 'Starting…'}</small>
    <small class="copy" title={bandsPath ?? ''}>Bands from {bandsPath?.replace(/^\/home\/[^/]+/, '~') ?? 'built-in list'}</small>
    <small>Colours: {theme.name ?? 'Omarchy theme'}</small>
  </div>
  <div class="field keys mono">
    <span class="lbl">Keys</span>
    <small>1–9 band · ← → next signal</small>
    <small>[ ] move · - + zoom · space listen</small>
    <small>f panel takes the screen · Esc back</small>
    <small>s settings · F11 full screen</small>
  </div>
{/snippet}

<style>
  .app {
    height: 100%;
    display: flex;
    min-height: 0;
  }
  .right {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 20px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--muted);
    background: linear-gradient(90deg, color-mix(in srgb, var(--hue) 13%, var(--bg-dark)), var(--bg-dark) 60%);
  }
  .freq {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .digits {
    font-size: 48px;
    font-weight: 600;
    letter-spacing: -1px;
    color: var(--fg-bright);
    font-variant-numeric: tabular-nums;
  }
  .digits em {
    font-style: normal;
    color: var(--hue);
  }
  .outside {
    align-self: center;
    padding: 1px 8px;
    border: 1px solid var(--yellow);
    border-radius: 10px;
    color: var(--yellow);
    font-size: 11px;
    white-space: nowrap;
  }
  .unit {
    color: var(--fg-dim);
    font-size: 12px;
  }
  .now {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1 1 200px;
    min-width: 0;
  }
  .now .title {
    font-weight: 600;
    font-size: 15px;
    color: var(--fg-bright);
  }
  .now .sub {
    color: var(--fg-dim);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .play {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    border: 1px solid var(--hue);
    background: transparent;
    color: var(--hue);
    font-weight: 600;
  }
  .play :global(svg) {
    fill: currentColor;
  }
  .play.on {
    background: var(--hue);
    color: var(--bg-darker);
  }
  .rec {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 50%;
    border: 1px solid var(--muted);
    background: transparent;
    color: var(--red);
  }
  .rec :global(svg) {
    fill: currentColor;
  }
  .rec.on {
    border-color: var(--red);
    background: var(--red);
    color: var(--bg-darker);
  }
  .rec-note {
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--fg-dim);
    font-size: 12px;
  }
  .rec-note.err {
    color: var(--red);
  }
  .ctl {
    display: flex;
    flex-direction: column;
    gap: 5px;
    width: 130px;
    color: var(--fg-dim);
    font-size: 11px;
  }
  .ctl input {
    width: 100%;
    margin: 0;
    accent-color: var(--hue);
  }
  .levels {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .levels .ctl {
    width: 170px;
  }
  .clocks {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    margin-left: auto;
    font-size: 13px;
  }
  .clocks span + span {
    color: var(--fg-dim);
  }
  .clocks small {
    font-size: 10px;
  }
  .track {
    position: relative;
    height: 8px;
    border-radius: 4px;
    background: var(--muted);
    cursor: ew-resize;
    touch-action: none;
  }
  .fillclip {
    position: absolute;
    inset: 0;
    overflow: hidden;
    border-radius: 4px;
  }
  /* slid, not resized, and not eased: a transition would keep the compositor running between the five updates a second */
  .fill {
    width: 100%;
    height: 8px;
    border-radius: 4px;
    background: var(--ramp, linear-gradient(90deg, var(--green), var(--yellow)));
  }
  .fill.muted {
    background: var(--fg-dim);
  }
  /* the squelch level; the handle you drag */
  .sq-mark {
    position: absolute;
    top: -4px;
    width: 4px;
    height: 16px;
    margin-left: -2px;
    border-radius: 2px;
    background: var(--fg-bright);
    box-shadow: 0 0 0 2px var(--bg-dark);
  }
  .sq-mark.off {
    opacity: 0.45;
  }
  .chip {
    padding: 2px 8px;
    border-radius: 6px;
    border: 0;
    font-size: 12px;
    font-weight: 600;
  }
  .chip.warn {
    color: var(--yellow);
    background: color-mix(in srgb, var(--yellow) 16%, transparent);
  }
  .chip.err {
    color: var(--red);
    background: color-mix(in srgb, var(--red) 16%, transparent);
  }
  .iconbtn {
    display: grid;
    place-items: center;
    min-width: 36px;
    min-height: 36px;
    border-radius: 8px;
    border: 0;
    background: transparent;
    color: var(--fg-dim);
  }
  .iconbtn:hover {
    background: var(--bg-light);
    color: var(--fg-bright);
  }
  /* waterfall and panel side by side; stacked in a narrow window; either can take the screen */
  main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
    padding: 10px;
  }
  main.has-panel {
    grid-template-columns: minmax(0, 1fr) 10px minmax(280px, var(--share));
  }
  main.has-panel.stacked {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) 10px minmax(120px, var(--share));
  }
  main.has-panel.max-panel {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: 96px 10px minmax(0, 1fr);
  }
  main.has-panel.max-wf {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) 10px auto;
  }
  .divider {
    position: relative;
    padding: 0;
    border: 0;
    background: transparent;
    cursor: col-resize;
    touch-action: none;
  }
  .stacked .divider,
  .max-panel .divider,
  .max-wf .divider {
    cursor: row-resize;
  }
  .max-panel .divider,
  .max-wf .divider {
    pointer-events: none;
  }
  .divider::after {
    content: '';
    position: absolute;
    inset: 0;
    margin: auto;
    width: 4px;
    height: 48px;
    border-radius: 2px;
    background: var(--muted);
    opacity: 0;
    transition: opacity 0.15s;
  }
  .stacked .divider::after {
    width: 48px;
    height: 4px;
  }
  main:hover .divider::after {
    opacity: 1;
  }
  .divider:hover::after,
  .divider:focus-visible::after {
    background: var(--hue);
  }
  .max-panel .divider::after,
  .max-wf .divider::after {
    display: none;
  }
  .box {
    min-width: 0;
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-darker);
  }
  .hint {
    position: absolute;
    left: 50%;
    bottom: 36px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 7px 8px 7px 14px;
    border-radius: 8px;
    border: 1px solid var(--muted);
    background: color-mix(in srgb, var(--bg-light) 94%, transparent);
    font-size: 13px;
    white-space: nowrap;
  }
  .hint button {
    border: 0;
    border-radius: 6px;
    padding: 4px 12px;
    background: var(--hue);
    color: var(--bg-darker);
    font-weight: 600;
  }
  .ptab {
    padding: 2px 8px;
    border-radius: 6px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
    color: var(--fg-dim);
    font-size: 12px;
    white-space: nowrap;
  }
  .ptab:hover {
    color: var(--fg-bright);
    border-color: var(--c);
  }
  .ptab b {
    color: var(--c);
  }
  /* panel bodies: one card style for balloons, HD, calls and passes */
  .pbody {
    flex: 1;
    min-height: 0;
    padding: 12px;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .pbody.wrap {
    display: flex;
    flex-wrap: wrap;
    align-content: flex-start;
    gap: 12px;
  }
  .col {
    flex: 1 1 220px;
    max-width: 320px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .col h2 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--fg-dim);
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
    text-align: left;
  }
  .item.balloon {
    flex: 0 1 260px;
    min-width: 200px;
  }
  .item.sel {
    border-color: var(--pane, var(--hue));
    background: color-mix(in srgb, var(--pane, var(--hue)) 10%, var(--bg-light));
  }
  .item .name {
    font-weight: 600;
    color: var(--fg-bright);
  }
  .item .big {
    font-size: 16px;
    font-weight: 600;
    color: var(--pane, var(--hue));
  }
  .item .dim,
  .empty {
    font-size: 12px;
    color: var(--fg-dim);
  }
  .empty {
    margin: 0;
    line-height: 1.45;
  }
  /* map and list: side by side when the panel is wide, map over list when it is narrow;
     the space station's world map is always on top at its own shape, passes below */
  .split {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .mapbox {
    position: relative;
    overflow: hidden;
    flex: 0 0 auto;
    background: var(--bg-darker);
  }
  .issmap {
    aspect-ratio: 2;
    width: 100%;
    max-width: 640px;
    align-self: center;
  }
  .repmap {
    flex: 1 1 55%;
    min-height: 0;
  }
  .list {
    flex: 1 1 0;
    min-height: 0;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
    overscroll-behavior: contain;
    border-top: 1px solid var(--muted);
  }
  .over .list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    align-content: start;
  }
  .over .list .empty {
    grid-column: 1 / -1;
  }
  @container (min-width: 620px) {
    .split:not(.over) {
      flex-direction: row;
    }
    .repmap {
      flex: 2 1 0;
      height: 100%;
    }
    .split:not(.over) .list {
      flex: 1 1 260px;
      border-top: 0;
      border-left: 1px solid var(--muted);
    }
  }
  .maplabel {
    position: absolute;
    left: 8px;
    bottom: 6px;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 12px;
    color: var(--fg-dim);
    background: color-mix(in srgb, var(--bg-darker) 80%, transparent);
  }
  /* settings drawer */
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
  }
  .field .lbl {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--fg-dim);
  }
  .field small {
    font-size: 12px;
    line-height: 1.4;
    color: var(--fg-dim);
  }
  .field.toggle {
    flex-direction: row;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .field.toggle span {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .field.toggle input {
    accent-color: var(--hue);
    width: 18px;
    height: 18px;
    margin-top: 2px;
  }
  .field select,
  input.loc {
    min-height: 34px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
  }
  input.loc.bad {
    border-color: var(--red);
  }
  .keys small {
    font-size: 11px;
  }
  .wf {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  /* the waterfall fills its box; left to size itself, the canvas would set its own height */
  .wf > :global(*) {
    position: absolute;
    inset: 0;
  }
  .lane {
    position: relative;
    flex: 0 0 auto;
  }
  .dot {
    position: absolute;
    bottom: 4px;
    width: 6px;
    height: 6px;
    margin-left: -3px;
    border-radius: 3px;
  }
  .dot.hidden {
    background: var(--yellow);
  }
  .dot.quiet {
    background: var(--muted);
  }
  .dot.steady {
    width: 4px;
    height: 4px;
    margin-left: -2px;
    background: var(--fg-dim);
    opacity: 0.5;
  }
  .stem {
    position: absolute;
    top: 62px;
    bottom: 0;
    width: 1px;
    background: var(--muted);
  }
  .stem.sel {
    background: var(--accent);
    box-shadow: 0 0 6px var(--accent);
  }
  .card {
    position: absolute;
    top: 6px;
    height: 56px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px 8px 7px;
    border-radius: 7px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
    text-align: left;
    overflow: hidden;
    transition: left 150ms;
  }
  .box.panning .card {
    transition: none;
  }
  .card.quiet:not(.sel) {
    opacity: 0.6;
    border-style: dashed;
  }
  .card.sel {
    z-index: 2;
    overflow: visible;
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, var(--bg-light));
    box-shadow:
      0 0 0 1px var(--accent),
      0 0 16px 3px color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .row1 {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
  }
  .ttl {
    flex: 1;
    font-size: 13px;
    font-weight: 600;
    color: var(--fg-bright);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .more {
    padding: 0 5px;
    border-radius: 4px;
    background: var(--muted);
    font-size: 10px;
  }
  .row2 {
    width: 100%;
    font-size: 10.5px;
    color: var(--fg-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row3 {
    display: flex;
    align-items: center;
    gap: 5px;
    width: 100%;
    font-size: 10px;
    color: var(--fg-dim);
  }
  .tag.saved {
    color: var(--cyan);
  }
  /* HD Radio has its own colour, orange, so a digital station is easy to spot */
  .hdchip {
    flex: none;
    padding: 0 6px;
    font-family: var(--mono);
    font-size: 10px;
    border-radius: 4px;
    border: 1px solid var(--orange);
    color: var(--orange);
    font-weight: 700;
    cursor: pointer;
    /* a steady glow: any running animation, even on the compositor, redraws the whole window 60 times a second here */
    box-shadow: 0 0 6px var(--orange);
  }
  .hdchip.on {
    background: var(--orange);
    color: var(--bg);
    box-shadow: 0 0 8px var(--orange);
  }
  .card.sel.hdon {
    border-color: var(--orange);
    background: color-mix(in srgb, var(--orange) 14%, var(--bg-light));
    box-shadow:
      0 0 0 1px var(--orange),
      0 0 16px 3px color-mix(in srgb, var(--orange) 50%, transparent);
  }
  /* one column: the station card, then traffic, then weather */
  .hdbody {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .hdbody .col {
    flex: none;
    max-width: 420px;
  }
  .hdpic {
    width: 100%;
    border-radius: 6px;
    margin-bottom: 4px;
  }
  .hdwait {
    display: grid;
    place-items: center;
    height: 120px;
    border-radius: 6px;
    color: var(--fg-dim);
    background: var(--bg-light);
  }
  /* a picture on its way; fills as its pieces arrive */
  .hdload {
    height: 4px;
    border-radius: 2px;
    overflow: hidden;
    background: var(--muted);
  }
  .hdload span {
    display: block;
    height: 100%;
    background: var(--orange);
    transition: width 0.4s;
  }
  .hdprogs {
    display: flex;
    gap: 6px;
  }
  .hdprogs button {
    flex: 1;
    padding: 4px 0;
    border-radius: 6px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
    color: var(--fg);
  }
  .hdprogs button.sel {
    border-color: var(--orange);
    color: var(--orange);
  }
  /* found → audio → quality, lit as each is reached; the words are in the tooltip */
  .hdstat {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
  }
  .hdstat .step {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 8px;
    color: var(--fg-dim);
    background: var(--bg-light);
    opacity: 0.45;
    transition: color 0.3s, background 0.3s, opacity 0.3s;
  }
  .hdstat .step.on {
    opacity: 1;
    color: var(--green);
    background: color-mix(in srgb, var(--green) 18%, transparent);
  }
  .hdstat .step.live {
    color: var(--orange);
    background: color-mix(in srgb, var(--orange) 22%, transparent);
    box-shadow: 0 0 12px color-mix(in srgb, var(--orange) 35%, transparent);
  }
  .hdstat .step.seek {
    animation: hdseek 1.6s ease-in-out infinite;
  }
  @keyframes hdseek {
    50% {
      opacity: 0.8;
    }
  }
  .hdgrid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    border-radius: 6px;
    overflow: hidden;
  }
  .hdgrid img,
  .hdgrid span {
    width: 100%;
    aspect-ratio: 1;
    display: block;
    background: var(--bg-light);
  }
  .bar {
    position: absolute;
    left: 8px;
    right: 8px;
    bottom: 3px;
  }
  .edge {
    position: absolute;
    min-height: 52px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 10px;
    border: 1px solid var(--accent);
    background: color-mix(in srgb, var(--bg-dark) 90%, transparent);
    color: var(--fg-bright);
  }
  .edge.left {
    left: 8px;
  }
  .edge.right {
    right: 8px;
  }
  .edge .arrow {
    font-size: 18px;
    color: var(--accent);
  }
  .edge .txt {
    display: flex;
    flex-direction: column;
    font-size: 13px;
  }
  .edge.right .txt {
    text-align: right;
  }
  .edge .txt span {
    font-size: 11px;
    color: var(--fg-dim);
  }
  .err {
    color: var(--red);
  }
  .warn {
    color: var(--yellow);
  }
  .reprow {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    text-align: left;
  }
  .reprow:hover {
    background: var(--bg-light);
  }
  .reprow.sel {
    background: color-mix(in srgb, var(--pane, var(--hue)) 14%, var(--bg-light));
  }
  .chlist {
    gap: 2px;
    border-top: 0;
  }
  .chgroup {
    margin: 10px 8px 4px;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--fg-dim);
  }
  .chgroup:first-child {
    margin-top: 2px;
  }
  .chrow {
    align-items: center;
    transition: background 0.3s;
  }
  .chrow .dot {
    position: relative;
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1px solid var(--muted);
  }
  .chrow.live {
    background: color-mix(in srgb, var(--green) 12%, transparent);
  }
  .chrow.live.sel {
    background: color-mix(in srgb, var(--green) 14%, color-mix(in srgb, var(--pane, var(--hue)) 14%, var(--bg-light)));
  }
  .chrow.live .name {
    color: var(--fg-bright);
    font-weight: 500;
  }
  .chrow.live .dot {
    border-color: var(--green);
    background: var(--green);
    /* a steady halo, not a pulse: a running animation redraws the whole window 60 times a second here */
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--green) 35%, transparent);
  }

  .reprow .town {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
