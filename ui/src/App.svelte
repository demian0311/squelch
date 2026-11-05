<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import * as api from './lib/api';
  import { channelAt, channelList, quietChannels, setRepeaters } from './lib/channels';
  import type { Band, BandPlan, GroundPoint, Iss, IssPass, Mode, Setup, Status, Theme } from './lib/api';
  import { MODE_NAMES, allBookmarks, bandAt, captureCenter, bookmarkNear, namedFrequencies, notOnAir, bandEntryHz, fmtHz, fmtMHz, label, modeAt, Persistence, placeCards, anchor, withoutDeadAir, fmChannel, rdsName, rdsNow, snapTune, withRds, sondeDrift, sondeName, sondeSummary, withSondes, type Saved, type Signal } from './lib/radio';
  import { apply as applyTheme, groupHue, lut, mix, rampGradient, resolve, waterfallStops } from './lib/theme';
  import { hdLabel, hdState, nextHd, trafficGrid } from './lib/hd';
  import { NO_PICKS, folded, groupRows, holdSay, p25Site, p25Summary, systemKey, type Picks } from './lib/p25';
  import { ACARS_CENTRE_HZ, LOG_DAYS, MAX_MESSAGES, acarsPosition, acarsSummary, acarsTime, acarsWho, filterMessages, joinBlocks, labelName, mergeMessages } from './lib/acars';
  import { ageDays, ageLabel, clock, compass, dayLabel, freshness, freshnessNote, groundAt, issRailLine, issTitle, localTime, utcTime, overhead, passColour, passWhy, RECEPTION_DB, until, upcoming } from './lib/iss';
  import BandRail from './components/BandRail.svelte';
  import Waterfall from './components/Waterfall.svelte';
  import BandStrip from './components/BandStrip.svelte';
  import EmsStrip from './components/EmsStrip.svelte';
  import { addLine, emsRanges, isEms, oneEms, railId, type LookLine } from './lib/ems';
  import StationList from './components/StationList.svelte';
  import { dialRows, localName, localStations, localType, noteHeard, stripTicks, withHeard, type Heard } from './lib/stations';
  import Pane from './components/Pane.svelte';
  import BookmarksScreen from './components/BookmarksScreen.svelte';
  import RecordingsScreen from './components/RecordingsScreen.svelte';
  import { clockLength, nextAfter, parseRecordingName, timeOfDay } from './lib/recordings';
  import AirplanesScreen from './components/AirplanesScreen.svelte';
  import EmsScreen from './components/EmsScreen.svelte';
  import Talkgroups from './components/Talkgroups.svelte';
  import { AIR_TURNS, oneAirplanes, type Talk } from './lib/airplanes';
  import { awakeUntil, nextWake, openWindows, reminders, when, withIds, type Slot, type Window } from './lib/schedule';
  import * as store from './lib/store';
  import IssMap from './components/IssMap.svelte';
  import RepeaterMap from './components/RepeaterMap.svelte';
  import AircraftMap from './components/AircraftMap.svelte';
  import { ADSB_CENTRE_HZ, TRAIL_KEEP_MS, adsbSummary, aircraftFacts, aircraftLine, aircraftRows, extendTrails, trailPath, filterFlights, flightDays, flightLine, flightName, isAlarm, mapFrame, pickedFirst, trackPath, type Trails } from './lib/adsb';
  import Level from './components/Level.svelte';
  import { DEFAULT_VIEW, REACH_KM, WIDEST_KM, heldOutputs, liveOutputs, repeaterPoints, type HeardOutput, type MapView } from './lib/repeaters';
  import Icon from './components/Icon.svelte';
  import ChannelBoard from './components/ChannelBoard.svelte';
  import BoardActivity from './components/BoardActivity.svelte';
  import { BOARDS, SHOW_AFTER_MIN, activity, boardFor, channelOn, listened, liveOn, parseHistory, record, sideView, sidesIn, spanText, type Minute } from './lib/board';

  const BANDWIDTH: Record<Mode, number> = { wfm: 200_000, nfm: 12_500, am: 8_000, usb: 2_800, lsb: 2_800, none: 10_000, p25: 12_500 };
  // a card's border and padding, its signal bar and its bookmark, each with its gap
  const CARD_PAD_PX = 15;
  const CARD_BAR_PX = 18;
  const CARD_KEPT_PX = 14;
  const CARD_HD_PX = 36;
  const CARD_MAX_PX = 240;
  // HD signal quality (nrsc5's MER) mapped onto the level bar, empty to full
  const HD_MER_DB = [2, 14];
  const HD_AUDIO_HOLD_MS = 3_000;
  // The signal cards give way before the waterfall does: as the box gets shorter they lose their second line, then
  // their third, then only the dots stay; the spectrum line shrinks from SPEC_H to SPEC_MIN in between.
  const LANES = [34, 14];
  // FM cards have a second line, the station's type
  const FM_LANES = [48, 14];
  const SPEC_H = 130;
  const SPEC_MIN = 48;
  // Waterfall.svelte's frequency axis and its smallest waterfall; BandStrip.svelte's height with its border
  const WF_AXIS = 22;
  const WF_MIN = 60;
  const BAND_STRIP_H = 31;
  // EmsStrip.svelte: scale, picture and words, with its border
  const EMS_STRIP_H = 80;
  // the waterfall's spectrum line while the panel has the screen
  const STRIP_SPEC_H = 24;
  const MIN_SPAN = 25_000;
  // a measurement past this is a bad one; cheap dongles are within about 100 ppm
  const MAX_PPM = 200;
  // the signal meter's full scale, dB above the noise floor
  const METER_DB = 40;
  // keep the listened-to frequency off the tuner's centre
  const DC_OFFSET = 60_000;

  let setup = $state.raw<Setup>({ sample_rate: 2_400_000, fft_size: 2048, below_floor_db: 8, range_db: 60 });
  let plan = $state.raw<BandPlan>({ groups: [], bands: [] });
  let bandsPath = $state<string | null>(null);
  let bandsError = $state<string | null>(null);
  let theme = $state.raw<Theme>(resolve({}));
  let status = $state.raw<Status | null>(null);

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
  // when the last recording ended, and whether its path was copied since
  let recEndS = $state(0);
  let recCopied = $state(false);
  // the Recordings screen: every saved file, and the row it opens on
  let recordingsOpen = $state(false);
  let recordingsList = $state.raw<api.SavedRecording[]>([]);
  let recFocus = $state<string | null>(null);
  // the recording playing in place of the live sound, and the one already played on from when it ended
  let play = $derived(status?.play ?? null);
  // the top bar, space and the sub-line are the recording's while it plays or its screen is open
  let recOwns = $derived(!!play && (!play.paused || recordingsOpen));
  let barPlay = $derived(recOwns ? play : null);
  let playedOn = $state<string | null>(null);
  /** How long the saved file's copy icon stays after a recording ends. */
  const SAVED_FOR_S = 5 * 60;
  let volume = $state(0.7);
  let gain = $state<string>('auto');
  // crystal correction: measured unless typed in by hand
  let ppm = $state(store.getSettings().crystal_ppm ?? store.getState('ppm.measured', 0));
  let ppmByHand = $state(store.getSettings().crystal_ppm != null);
  let railOpen = $state(store.getState('rail', true));
  let settingsOpen = $state(false);
  // dB above the noise floor; 0 means off. Empty channels read 3–10 dB on this dongle.
  const DEFAULT_SQUELCH = 12;
  let squelch = $state(store.getSettings().squelch ?? DEFAULT_SQUELCH);
  // settings.toml does not parse or save: shown with the other problems, and the file is left alone
  let settingsError = $state(store.startupSettings().error);
  let settingsPath = store.startupSettings().path;
  store.onProblem((m) => (settingsError = m));
  // bands that rarely give us anything stay off the rail until someone wants to investigate them
  let showDeadAir = $state(store.getSettings().show_dead_air ?? false);
  // bookmarks keep the two EMS bands apart; the rail and its shortcuts show one entry
  let bookmarkPlan = $derived(oneAirplanes(showDeadAir ? plan : withoutDeadAir(plan)));
  let railPlan = $derived(oneEms(bookmarkPlan));
  function toggleDeadAir() {
    showDeadAir = !showDeadAir;
    store.setSetting('show_dead_air', showDeadAir);
  }
  /** HD Radio program chosen per FM station (by channel Hz); a station without an entry plays analog. */
  let hdPrefs = $state<Record<string, number>>(store.getState('hd', {}));
  /** Last frequency used in each band, so returning to a band picks up where it left off. */
  const lastHz: Record<string, number> = store.getState('lastHz', {});
  function toggleRail() {
    railOpen = !railOpen;
    store.setState('rail', railOpen);
  }
  let full = $state(false);
  let boxW = $state(1000);
  // while the view itself moves, cards jump with it; the slide is only for cards appearing or folding
  let panning = $state(false);
  let panTimer: ReturnType<typeof setTimeout> | undefined;
  let waterfall: Waterfall;
  /** Station data per FM channel, kept after tuning away. */
  let rdsHeard = $state(new Map<number, api.Rds>());
  /** Stations heard per band, across runs, for the band strip; `stations` in the state file. */
  let heard = $state.raw<Record<string, Heard[]>>(store.getState('stations', {}));
  /** Ticks heard before this are from an earlier run, and the station may be gone. */
  const startedAt = Date.now();
  const persist = new Persistence();
  // status arrives every 200 ms, so a 15 s window holds 75 updates
  const UPDATES_PER_WINDOW = 75;
  let showSteady = $state(false);

  let saved = $derived<Saved[]>(allBookmarks(plan));
  let named = $derived(namedFrequencies(plan, saved));
  let band = $derived(plan.bands.find((b) => b.id === bandId));
  // All of EMS above the waterfall: the engine hops across it now and then, and a line comes back in the status
  let emsLook = $derived(emsRanges(plan, band));
  let emsLookKey = $derived(JSON.stringify(emsLook));
  let emsLines = $state.raw<LookLine[]>([]);
  let emsLineAt = $state<number | null>(null);
  let emsSide = $state<string | null>(null);
  // the trunking side of EMS last used, where "Back to the towers" goes
  let emsTowerSide = $state('ps800');
  $effect(() => {
    if (isEms(band) && band!.mode === 'p25') emsTowerSide = band!.id;
    if (isEms(band)) emsSide = band!.id;
  });
  $effect(() => {
    void emsLookKey;
    api.setBandLook(untrack(() => emsLook));
  });
  let rate = $derived(setup.sample_rate);
  let maxSpan = $derived(rate * 0.8);
  let span = $derived(ve - vs);
  let boxH = $state(0);
  let fit = $derived.by(() => {
    const lanes = band?.id === 'fm' ? FM_LANES : LANES;
    if (!boxH) return { level: 0, lane: lanes[0], spec: SPEC_H };
    const room = boxH - (band?.id === 'fm' ? BAND_STRIP_H : emsLook ? EMS_STRIP_H : 0) - WF_AXIS - WF_MIN;
    const at = lanes.findIndex((h) => room - h >= SPEC_MIN);
    const level = at < 0 ? lanes.length - 1 : at;
    return { level, lane: lanes[level], spec: Math.max(STRIP_SPEC_H, Math.min(SPEC_H, room - lanes[level])) };
  });

  // ── tuning ──────────────────────────────────────────

  let pending = 0;
  function send() {
    if (pending) return;
    pending = requestAnimationFrame(() => {
      pending = 0;
      api.tune(center, tuned);
    });
  }

  /** Move the dongle only when the view or the tuned frequency leaves the usable capture. */
  function recenter() {
    const half = rate * 0.45;
    // ACARS listens on channels, not on what is in view: stay where the most of them fit while the view allows it
    const fitsAcars = [vs, ve, tuned].every((f) => Math.abs(f - ACARS_CENTRE_HZ) <= half);
    // a control-channel scan owns the dongle until a hand tune (the engine's tuned frequency still matches)
    const scanning = !!p25?.scan && status !== null && Math.abs(status.tuned_hz - tuned) < 1;
    center = scanning ? status!.center_hz : band?.decoder === 'acars' && fitsAcars ? ACARS_CENTRE_HZ : captureCenter(center, vs, ve, tuned, half, DC_OFFSET);
    send();
  }

  // the scan hops the dongle across the band: the waterfall follows each hop
  $effect(() => {
    const c = status?.center_hz;
    if (!p25?.scan || c === undefined || Math.abs(c - untrack(() => center)) < 1) return;
    untrack(() => {
      center = c;
      setView(c - span / 2, c + span / 2);
    });
  });

  // the control channel a scan found becomes the tuned frequency, so the band remembers it; the engine keeps the site
  $effect(() => {
    const hz = p25?.control_hz;
    if (hz === null || hz === undefined || p25?.state === 'searching' || airWatching || Math.abs(hz - untrack(() => tuned)) < 5_000) return;
    untrack(() => {
      if (status) center = status.center_hz;
      tuneCentered(hz);
    });
  });

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
    // anything tuning by hand while the planes screen owns the radio means they want the radio
    if (airWatching) seeRadio = true;
    tuned = Math.round(hz);
    if (tuned < vs || tuned > ve) {
      // `span` follows `vs`: read once, or a long jump turns the view backwards
      const s = span;
      vs = tuned - s / 2;
      ve = tuned + s / 2;
    }
    if (!keepMode) {
      const m = modeAt(plan, named, tuned, mode);
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

  const tuneSnapped = (hz: number) => tuneCentered(snapTune(hz, status?.signals ?? [], named, span / boxW, mode, band?.id));

  /** A click on a frequency: tune it and bring it to the middle; a channel on a board shows its whole side. */
  function tuneClick(hz: number) {
    if (!board || !channelOn(board, hz)) return tuneCentered(hz);
    tuneTo(hz);
    const [a, b] = sideView(board, hz);
    slideView(a, b);
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
    store.setState('lastHz', lastHz);
  });

  let outside = $derived(!!band && (tuned < band.low_mhz * 1e6 || tuned > band.high_mhz * 1e6));

  function pickBand(picked: Band) {
    // the one EMS entry goes back to the side of EMS last tuned
    const b = isEms(picked) ? (plan.bands.find((x) => x.id === emsSide) ?? picked) : picked;
    bookmarksOpen = false;
    leaveRecordings();
    seeRadio = false;
    airListen = null;
    bandId = b.id;
    panelPick = null;
    const s = Math.min(maxSpan, (b.high_mhz - b.low_mhz) * 1e6);
    const entering = boardFor(b.id);
    const start = lastHz[b.id] === undefined && entering?.home ? entering.home : bandEntryHz(b, lastHz);
    let nvs = start - s / 2;
    nvs = Math.max(b.low_mhz * 1e6, Math.min(nvs, b.high_mhz * 1e6 - s));
    if (s >= (b.high_mhz - b.low_mhz) * 1e6) nvs = (b.low_mhz * 1e6 + b.high_mhz * 1e6) / 2 - s / 2;
    vs = nvs;
    ve = nvs + s;
    // a band of channels: just the channels, not the empty air around them
    if (entering) [vs, ve] = sideView(entering, start);
    tuned = Math.round(start);
    mode = modeAt(plan, named, tuned, b.mode);
    api.setMode(mode);
    recenter();
  }

  // ── signals ─────────────────────────────────────────

  // the FCC's stations near the location; with them, memory counts only where it agrees
  let fmNearby = $state.raw<api.FmStation[]>([]);
  $effect(() => {
    const at = location;
    api.fmStations(at).then((list) => {
      if (at === location) fmNearby = list;
    });
  });
  let fmLocal = $derived(localStations(fmNearby, heard.fm ?? [], rdsHeard));
  const stationName = (hz: number) => localName(fmNearby.length > 0, fmLocal, heard.fm ?? [], hz);

  let signals = $derived<(Signal & { steady: boolean })[]>(
    withSondes(
      withHeard(
        withRds(
          (status?.signals ?? []).map((d) => label(plan, named, d)),
          rdsHeard,
        ),
        (hz) => stationName(hz),
      ),
      status?.sondes ?? [],
    ).map((s) => ({ ...s, steady: persist.steady(s, Date.now(), UPDATES_PER_WINDOW) })),
  );
  let steadyCount = $derived(signals.filter((s) => s.steady && s.hz >= vs && s.hz <= ve).length);
  let listening = $derived(signals.find((s) => Math.abs(s.hz - tuned) <= Math.max(s.widthHz / 2, 3_000)));
  let listeningMark = $derived(named.find((s) => Math.abs(s.mhz * 1e6 - tuned) < 1_500));

  // placing cards must not depend on laying them out, so a title is measured on a canvas; the ttl style's font
  const titleWidths = new Map<string, number>();
  let titlePen: CanvasRenderingContext2D | null = null;
  function titleWidth(title: string) {
    let w = titleWidths.get(title);
    if (w === undefined) {
      titlePen ??= document.createElement('canvas').getContext('2d')!;
      titlePen.font = `600 12px ${getComputedStyle(document.body).fontFamily}`;
      w = Math.ceil(titlePen.measureText(title).width);
      titleWidths.set(title, w);
    }
    return w;
  }

  // an FM card's second line: what the station says it plays, from memory until RDS comes in
  const stationType = (hz: number) => (mode === 'wfm' ? (rdsHeard.get(fmChannel(hz))?.pty ?? localType(fmNearby.length > 0, fmLocal, heard.fm ?? [], hz) ?? null) : null);

  // last frame's card frequencies, so cards don't follow every wobble of a measured centre
  let anchors: number[] = [];
  let cards = $derived.by(() => {
    const heard = signals.filter((s) => s.hz >= vs && s.hz <= ve && (showSteady || !s.steady));
    const isTuned = (s: Signal) => s === listening || (!!s.quiet && Math.abs(s.hz - tuned) < 1_000);
    // a card means on air, bookmarked or special, in that order; an ordinary channel gets one only once zooming leaves room for all of them
    const cardType = (s: Signal) => (s.quiet && !isTuned(s) ? null : stationType(s.hz));
    const cardW = (s: Signal) => {
      const first = titleWidth(s.title) + CARD_PAD_PX + (s.quiet ? 0 : CARD_BAR_PX) + (bookmarkNear(saved, s.hz, Math.max(s.widthHz / 2, 5_000)) ? CARD_KEPT_PX : 0) + (isTuned(s) && mode === 'wfm' && (hd || status?.hd_sidebands || hdProgram !== null) ? CARD_HD_PX : 0);
      const type = cardType(s);
      return Math.min(CARD_MAX_PX, type ? Math.max(first, titleWidth(type) + CARD_PAD_PX) : first);
    };
    const quietCard = (hz: number, title: string, snrDb: number, isSaved: boolean) => ({ hz, widthHz: 0, snrDb, title, sub: 'Nothing on it now', tag: 'QUIET', saved: isSaved, channelHz: hz, quiet: true, steady: false });
    const yours = notOnAir(saved, vs, ve, heard).map((m) => quietCard(m.mhz * 1e6, m.name, -1, true));
    const special = notOnAir(named, vs, ve, heard)
      .filter((m) => !bookmarkNear(saved, m.mhz * 1e6, 1_000))
      .map((m) => quietCard(m.mhz * 1e6, m.name, -2, false));
    const ordinary = quietChannels(band?.id, vs, ve, heard)
      .filter((c) => !bookmarkNear(named, c.hz, 5_000))
      .map((c) => quietCard(c.hz, c.name, -3, false));
    const roomy = ordinary.reduce((w, c) => w + cardW(c) + 6, 0) <= boxW;
    const inView = [...heard, ...yours, ...special, ...(roomy ? ordinary : [])];
    const pxHz = span / boxW;
    const minSep = (a: Signal, b: Signal) => ((cardW(a) + cardW(b)) / 2 + 6) * pxHz;
    const placed = placeCards<Signal & { steady: boolean }>(inView, minSep, isTuned).map((p) => {
      const at =
        p.item === listening ? tuned : p.item.channelHz !== undefined ? p.item.channelHz : band?.mode === 'wfm' ? fmChannel(p.item.hz) : anchor(p.item.hz, anchors, Math.max(p.item.widthHz / 2, 10 * pxHz));
      const x = ((at - vs) / span) * boxW;
      return {
        ...p,
        at,
        x,
        w: cardW(p.item),
        type: cardType(p.item),
        meter: Math.max(0, Math.min(100, Math.round((p.item.snrDb / 40) * 100))),
        selected: isTuned(p.item),
        mark: bookmarkNear(saved, at, Math.max(p.item.widthHz / 2, 5_000)),
      };
    });
    anchors = placed.map((c) => c.at);
    return placed;
  });

  const cardLeft = (x: number, w: number) => Math.min(Math.max(x - w / 2, 6), boxW - w - 6);

  let dots = $derived.by(() => {
    const hidden = cards.flatMap((c) =>
      c.hidden.map((h) => ({ hz: h.hz, x: ((h.hz - vs) / span) * boxW, cls: h.quiet ? 'quiet' : 'hidden', title: `${fmtMHz(h.hz)} ${h.title} — ${h.quiet ? 'quiet' : 'no room for a card'}; click to tune` })),
    );
    const steady = showSteady
      ? []
      : signals
          .filter((s) => s.steady && s.hz >= vs && s.hz <= ve)
          .map((s) => ({ hz: s.hz, x: ((s.hz - vs) / span) * boxW, cls: 'steady', title: `${fmtMHz(s.hz)} — steady carrier, probably interference; click to tune` }));
    return [...hidden, ...steady];
  });

  let stripMarks = $derived(band?.id === 'fm' ? stripTicks(heard.fm ?? [], saved.filter((x) => x.band.id === 'fm'), startedAt, band.low_mhz * 1e6, band.high_mhz * 1e6) : []);

  function step(dir: 1 | -1) {
    const heard = signals.filter((s) => showSteady || !s.steady).map((s) => s.hz);
    const lo = band ? band.low_mhz * 1e6 : -Infinity;
    const hi = band ? band.high_mhz * 1e6 : Infinity;
    const marks = named.map((s) => s.mhz * 1e6).filter((hz) => hz >= lo && hz <= hi);
    const pick = (list: number[]) =>
      dir > 0 ? list.filter((f) => f > tuned + 1_000).sort((a, b) => a - b)[0] : list.filter((f) => f < tuned - 1_000).sort((a, b) => b - a)[0];
    const next = pick(heard) ?? pick(marks);
    if (next !== undefined) tuneTo(snapTune(next, status?.signals ?? [], named, span / boxW, mode, band?.id));
  }

  // ── location ────────────────────────────────────
  // one setting for everything that depends on where you are: station passes, nearby repeaters

  let location = $state(store.getSettings().location ?? '');
  function setLocation(v: string) {
    location = v.trim();
    store.setSetting('location', location || null);
    loadIss();
    loadRepeaters();
  }

  let repeaters = $state.raw<api.Repeater[]>([]);
  async function loadRepeaters() {
    mapFor = null;
    repeaters = await api.repeaters(location);
    setRepeaters(repeaters, plan.bands);
    // channel lookups aren't reactive; a new plan object recomputes everything that uses them
    plan = { ...plan };
  }

  // ── space station ───────────────────────────────

  let iss = $state.raw<Iss | null>(null);
  let nowS = $state(Date.now() / 1000);
  // the record button blinks once a second while recording: a steady style per tick, not an animation
  let recLit = $derived(!!rec?.recording && Math.floor(nowS) % 2 === 0);
  let issAge = $derived(ageDays(iss?.tle_epoch ?? null, nowS));
  let issUp = $derived(overhead(iss?.passes ?? [], nowS, issAge));
  let issNext = $derived(upcoming(iss?.passes ?? [], nowS).slice(0, 8));
  let issNote = $derived(freshnessNote(issAge));
  let railMarks = $derived<Record<string, { live: boolean; title: string; line?: string }>>(
    iss?.passes.length ? { iss: { live: issUp !== null, title: issTitle(iss.passes, nowS, issAge), line: issRailLine(iss.passes, nowS, issAge) } } : {},
  );

  // Ground track for the map; it covers 100 min ahead, so it is reloaded well before it runs out.
  let issTrack = $state.raw<GroundPoint[]>([]);
  // The pass under the pointer (or focused): the map draws it instead of the live position.
  let issHover = $state.raw<IssPass | null>(null);
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
  // the map's list: fetched the first time a band with a map opens, kept until the location or the file changes
  let mapRepeaters = $state.raw<api.Repeater[]>([]);
  let mapFor = $state<string | null>(null);
  let mapLoaded = $state(false);
  $effect(() => {
    if (repReachKm === undefined || mapFor === location) return;
    const at = location;
    mapFor = at;
    mapLoaded = false;
    api.repeatersMap(at).then((list) => {
      if (at !== mapFor) return;
      mapRepeaters = list;
      mapLoaded = true;
    });
  });
  let repInBand = $derived(band && repReachKm !== undefined ? mapRepeaters.filter((r) => r.mhz >= band.low_mhz && r.mhz <= band.high_mhz) : []);
  let repPoints = $derived(band && iss?.site ? repeaterPoints(repInBand, band, iss.site) : []);
  let repTuned = $derived(repPoints.find((p) => Math.abs(p.hz - tuned) < 2_500) ?? null);
  let repHeard = $state(new Map<number, HeardOutput>());
  $effect(() => {
    const live = liveOutputs(repPoints, status?.signals ?? [], squelch);
    repHeard = heldOutputs(untrack(() => repHeard), live, Date.now());
  });
  let repLive = $derived(new Map([...repHeard].map(([hz, h]) => [hz, h.snr])));
  // where the map looks: every run opens fully zoomed out
  let repView = $state<MapView>(DEFAULT_VIEW);
  const setRepView = (v: MapView) => (repView = v);
  let repSummary = $derived.by(() => {
    if (!iss?.site) return 'Set your location to see which are near';
    if (!repInBand.length) return 'None near your location';
    const shown = repPoints.filter((p) => p.km <= WIDEST_KM);
    const hear = shown.filter((p) => p.reach && !p.digital).length;
    const maybe = shown.filter((p) => p.digital).length;
    return `${shown.length} within ${WIDEST_KM} km · green: ${hear} you should hear · ${repLive.size} on air` + (maybe ? ` · blue: ${maybe} may be digital only` : '');
  });
  let locationInput = $state<HTMLInputElement | undefined>();
  /** The map title's pin: Settings, with the location ready to type over. */
  async function changeLocation() {
    settingsOpen = true;
    await tick();
    locationInput?.focus({ preventScroll: true });
    locationInput?.select();
  }

  // ── weather balloons ────────────────────────────

  let decoding = $derived(band?.decoder === 'radiosonde');
  let sondes = $derived(status?.sondes ?? []);
  let sondeLatest = $derived(sondes.reduce<(typeof sondes)[number] | null>((a, o) => (a && a.age_s <= o.age_s ? a : o), null));
  let sondeLine = $derived(sondeLatest ? `${sondes.length} heard · latest ${sondeName(sondeLatest)}, ${sondeSummary(sondeLatest)}` : 'None decoded yet');
  $effect(() => {
    api.setSondes(decoding);
  });

  // ── aircraft messages ───────────────────────────

  // The Airplanes screen, on the voice band's entry. Watching, the dongle takes turns between planes and messages;
  // listening, it sits on one channel and the screen says the map is paused; `seeRadio` is the ordinary voice band.
  let seeRadio = $state(false);
  let airListen = $state<Talk | null>(null);
  let planesScreen = $derived(band?.id === 'air' && !seeRadio);
  // The EMS screen: the receiver's steps, a verdict, the tower's channels and the talkgroups; `seeRadio` is the waterfall.
  let emsScreen = $derived(isEms(band) && !seeRadio);
  let airWatching = $derived(planesScreen && !airListen);
  // while watching, the waterfall shows the whole capture wherever the turns have put the dongle
  let airCentre = $derived(status?.center_hz ?? ACARS_CENTRE_HZ);
  let airVs = $derived(airCentre - rate * 0.4);
  let airVe = $derived(airCentre + rate * 0.4);
  // the big digits say where the radio is, which while watching is wherever the turn has put it
  let shownHz = $derived(airWatching ? airCentre : tuned);
  let onPlanesTurn = $derived(Math.abs(airCentre - ADSB_CENTRE_HZ) < rate / 2);
  $effect(() => {
    api.setAirTurns(airWatching ? AIR_TURNS : null);
  });
  let airWasWatching = false;
  $effect(() => {
    const on = airWatching;
    if (band?.id === 'air') api.setMode(on ? 'none' : untrack(() => mode));
    // the turns held the dongle: put it back where the page thinks it is
    if (airWasWatching && !on) untrack(recenter);
    airWasWatching = on;
  });
  function listenTo(t: Talk) {
    airListen = t;
    tuned = Math.round(t.channel.mhz * 1e6);
    mode = 'am';
    vs = tuned - span / 2;
    ve = tuned + span / 2;
    if (!playing) togglePlay();
  }

  let acarsOn = $derived(band?.decoder === 'acars' || airWatching);
  // the engine sends only the last few seconds; the list is kept here
  let acarsList = $state.raw<api.AcarsMessage[]>([]);
  let acarsQuery = $state('');
  let acarsShown = $derived(filterMessages(joinBlocks(acarsList), acarsQuery));
  $effect(() => {
    const incoming = status?.acars?.messages;
    if (incoming?.length) acarsList = mergeMessages(untrack(() => acarsList), incoming);
  });
  // what earlier runs heard, once, the first time an aircraft band opens
  let airLogLoaded = false;
  $effect(() => {
    if (!(acarsOn || adsbOn) || airLogLoaded) return;
    airLogLoaded = true;
    api.airMessages(LOG_DAYS, MAX_MESSAGES).then((old) => (acarsList = mergeMessages(untrack(() => acarsList), old)));
    api.airTracks(Date.now() - TRAIL_KEEP_MS).then((t) => (trails = { ...t, ...untrack(() => trails) }));
  });
  $effect(() => {
    api.setAcars(acarsOn);
  });

  // ── aircraft ────────────────────────────────────

  let adsbOn = $derived(band?.decoder === 'adsb' || airWatching);
  let adsb = $derived(status?.adsb ?? null);
  let planeRows = $derived(aircraftRows(adsb?.aircraft ?? [], iss?.site ?? null));
  let planeMap = $derived(mapFrame(adsb?.aircraft ?? [], iss?.site ?? null));
  let adsbLine = $derived(adsbSummary(adsb));
  $effect(() => {
    api.setAdsb(adsbOn);
  });
  $effect(() => {
    api.setSite(iss?.site?.lat ?? null, iss?.site?.lon ?? null);
  });
  // where each has been: from the log at first, then from each status
  let trails = $state.raw<Trails>({});
  $effect(() => {
    const list = adsb?.aircraft;
    if (list) trails = extendTrails(untrack(() => trails), list, Date.now());
  });
  let trailPaths = $derived.by(() => {
    if (!planeMap) return {};
    const c = planeMap.centre;
    return Object.fromEntries(Object.entries(trails).map(([k, v]) => [k, trailPath(v, c)]));
  });
  let planePick = $state<string | null>(null);
  let pickedPlane = $derived(planeRows.find((r) => r.a.icao === planePick)?.a ?? null);
  // messages from the same airframe, matched on the registration both carry
  let pickedMessages = $derived(pickedPlane?.registration ? acarsList.filter((m) => m.registration === pickedPlane?.registration).slice(0, 5) : []);
  const pickPlane = (icao: string) => (planePick = planePick === icao ? null : icao);
  let planeList = $state<HTMLElement>();
  $effect(() => {
    if (planePick) planeList?.scrollTo(0, 0);
  });

  // the log: every flight heard, across runs
  const FLIGHT_LOG_DAYS = 60;
  const FLIGHT_REFRESH_MS = 60_000;
  let flightList = $state.raw<api.Flight[]>([]);
  let flightQuery = $state('');
  let flightShown = $derived(flightDays(filterFlights(flightList, flightQuery), Date.now()));
  let flightPick = $state.raw<api.Flight | null>(null);
  let flightTrack = $state.raw<api.TrackPoint[]>([]);
  let flightMap = $derived(flightPick && flightTrack.length ? mapFrame([], iss?.site ?? null, flightTrack) : null);
  let flightPath = $derived(flightPick && flightMap ? { [flightPick.icao]: trackPath(flightTrack, flightMap.centre) } : {});
  function pickFlight(f: api.Flight) {
    if (flightPick && flightPick.icao === f.icao && flightPick.first_ms === f.first_ms) {
      flightPick = null;
      flightTrack = [];
      return;
    }
    flightPick = f;
    api.airTrack(f.icao, f.first_ms, f.last_ms).then((t) => (flightTrack = t));
  }

  // weather services launch twice a day at about 00:00 and 12:00 UTC
  const launchTimes = [0, 12]
    .map((h) => {
      const d = new Date();
      d.setUTCHours(h, 0, 0, 0);
      return d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
    })
    .join(' and ');

  // a date only on messages from other days
  const acarsDay = (ms: number) => (new Date(ms).toDateString() === new Date().toDateString() ? '' : `${new Date(ms).toLocaleDateString([], { month: 'short', day: 'numeric' })} `);
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
  let hdImages = $state.raw<api.HdImage[]>([]);
  let hdPicture = $derived(hdImages.find((i) => i.kind === 'art') ?? hdImages.find((i) => i.kind === 'logo'));
  let hdTraffic = $derived(trafficGrid(hdImages));
  let hdWeather = $derived(hdImages.find((i) => i.kind === 'weather'));
  let p25 = $derived(status?.mode === 'p25' ? (status.p25 ?? null) : null);
  // ── P25 picks: the one talkgroup that plays and those that never do, per system ──
  let p25Picks = $state<Record<string, Picks>>(store.getState('p25.picks', {}));
  let picks = $derived(p25Picks[systemKey(p25) ?? ''] ?? NO_PICKS);
  let tgRows = $derived(p25 ? groupRows(p25.groups, picks) : []);
  let fold = $derived(p25 ? folded(p25.groups) : { encrypted: null, tdma: null });
  $effect(() => {
    api.setP25Picks(picks.hold, picks.skip).catch(() => {});
  });
  // when the lock came: the control channel's own frequency changes, or the search ends
  let lockAt = $state(0);
  $effect(() => {
    const hz = p25?.control_hz;
    const searching = p25?.state === 'searching';
    untrack(() => {
      if (hz === null || hz === undefined || searching) lockAt = 0;
      else if (!lockAt) lockAt = nowS;
    });
  });
  let lockSecs = $derived(lockAt ? Math.max(0, nowS - lockAt) : 0);
  let otherEmsBand = $derived(plan.bands.find((b) => isEms(b) && b.mode === 'p25' && b.id !== band?.id));
  // on a plain-FM EMS band the screen shows what is tuned instead of a tower
  let emsAnalog = $derived(emsScreen && band?.mode === 'nfm' ? { tuned, open: status?.squelch_open ?? null, lo: band.low_mhz * 1e6, hi: band.high_mhz * 1e6 } : null);
  /** Plain voice the look found: tune it as narrow FM on whichever EMS band holds it. */
  function listenPlain(hz: number) {
    const b = bandAt(plan, hz);
    if (b && isEms(b)) emsSide = b.id;
    goTo(hz, 'nfm');
  }
  function backToTowers() {
    const b = plan.bands.find((x) => x.id === emsTowerSide) ?? plan.bands.find((x) => isEms(x) && x.mode === 'p25');
    if (!b) return;
    emsSide = b.id;
    pickBand(b);
  }
  /** A quiet tower: start the search again on the other EMS band, at its own start. */
  function goOtherEmsBand() {
    const b = otherEmsBand;
    if (!b) return;
    delete lastHz[b.id];
    emsSide = b.id;
    pickBand(b);
  }
  function setPicks(next: Picks) {
    const k = systemKey(p25);
    if (k === null) return;
    p25Picks = { ...p25Picks, [k]: next };
    store.setState('p25.picks', p25Picks);
  }
  /** Click a row: only that talkgroup plays; click it again and every talkgroup plays. */
  function holdGroup(g: number) {
    setPicks({ hold: picks.hold === g ? null : g, skip: picks.skip.filter((x) => x !== g) });
  }
  /** A skipped talkgroup never plays; skipping the held one releases the hold. */
  function skipGroup(g: number) {
    const on = picks.skip.includes(g);
    setPicks({ hold: picks.hold === g ? null : picks.hold, skip: on ? picks.skip.filter((x) => x !== g) : [...picks.skip, g] });
  }
  // digital audio drops out for a moment on a weak signal; hold the speaker lit through that
  let hdAudioAt = $state(0);
  let hdAudio = $derived(!!hd && (hd.digital || nowS * 1000 - hdAudioAt < HD_AUDIO_HOLD_MS));
  function setHdProgram(p: number | null) {
    const key = String(fmChannel(tuned));
    const next = { ...hdPrefs };
    if (p === null) delete next[key];
    else next[key] = p;
    hdPrefs = next;
    store.setState('hd', next);
  }
  function toggleHd(e: Event) {
    e.stopPropagation();
    setHdProgram(nextHd(hdProgram, hd?.programs ?? []));
  }
  let tunedChannel = $derived(channelAt(band?.id, tuned));
  let playTitle = $derived.by(() => {
    if (!recOwns || !play) return null;
    const p = parseRecordingName(play.file);
    return p ? (recordingName(p.hz, p.mode) ?? `${fmtMHz(p.hz)} MHz`) : 'Recording';
  });
  let playSub = $derived.by(() => {
    if (!recOwns || !play) return null;
    const p = parseRecordingName(play.file);
    const when = p ? `${timeOfDay(p.at)} ${dayWord(p.at)}` : 'a recording';
    const state = play.done ? 'ended' : play.paused ? 'paused · the live sound plays meanwhile' : 'playing · live sound waits';
    return `The recording from ${when} · ${clockLength(play.secs)} of ${clockLength(play.len_secs)} · ${state} · Esc stops`;
  });
  function dayWord(at: number) {
    const d = new Date(at * 1000);
    const t = new Date(nowS * 1000);
    const same = d.getFullYear() === t.getFullYear() && d.getMonth() === t.getMonth() && d.getDate() === t.getDate();
    return same ? 'today' : d.toLocaleDateString(undefined, { day: 'numeric', month: 'long' });
  }
  let nowTitle = $derived(playTitle ?? airListen?.name ?? (planesScreen ? 'Airplanes' : undefined) ?? (emsScreen ? (band?.name ?? 'EMS') : undefined) ?? (rds && rdsName(rds)) ?? listeningMark?.name ?? (mode === 'wfm' ? stationName(tuned) : undefined) ?? (listening?.byFreq ? null : listening?.title) ?? tunedChannel?.name ?? (band ? band.name : 'Tuned'));
  let muted = $derived(playing && status?.squelch_open === false);
  let nowSub = $derived(
    playSub ? playSub :
    airWatching ? `${onPlanesTurn ? 'Listening for planes' : 'Listening for messages'} · ${adsb?.aircraft.length ?? 0} overhead` :
    p25 && p25.state === 'searching' ? p25Summary(p25) :
    p25 && !p25.call && picks.hold !== null ? holdSay(p25, picks.hold) :
    muted ? 'Muted — signal is below the squelch level' :
    airListen ? 'Every plane on this channel, not only one' :
    hd?.digital && hd.title ? [hd.title, hd.artist].filter(Boolean).join(' — ') :
    rds && rdsNow(rds) ? rdsNow(rds)! : listening ? listening.sub : listeningMark ? 'Quiet right now' : 'Between signals — press → for the next one',
  );
  let meter = $derived(playing ? Math.max(0, Math.min(100, ((status?.channel_snr_db ?? 0) / METER_DB) * 100)) : 0);

  // ── theme ───────────────────────────────────────────

  let hues = $derived(Object.fromEntries(plan.groups.map((g) => [g.id, groupHue(theme, g.id, plan.groups.map((x) => x.id))])));
  let hue = $derived((band && hues[band.group]) ?? theme.accent);
  let stops = $derived(waterfallStops(theme));
  let stripPalette = $derived(lut(stops));
  // every strength bar in the page wears the waterfall's ramp on the meter's dB scale
  let ramp = $derived(rampGradient(stops, METER_DB, setup.below_floor_db, setup.range_db));
  // the local maps' shaded terrain (repeaters and airplanes alike), in theme colours; flat country has no relief to show,
  // so land and water get a trace of colour
  let mapPalette = $derived({ plain: mix(theme.lighter_background, theme.green, 0.1), peak: mix(theme.muted, theme.light_foreground, 0.15), shadow: theme.darker_background, light: theme.light_foreground, water: mix(theme.dark_background, theme.blue, 0.16) });
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

  type PanelId = 'sondes' | 'acars' | 'aircraft' | 'flights' | 'stations' | 'hd' | 'p25' | 'iss' | 'repeaters' | 'board' | 'activity' | 'channels';
  let panelPick = $state<PanelId | null>(null);
  // every saved frequency, over the waterfall and its panel
  let bookmarksOpen = $state(false);

  // Times to tune in. The list is kept in schedule.toml; a reminder shows in the header from LEAD_MS
  // before a start until it is dismissed or a grace period after, and goes to the desktop once per start.
  let slots = $state<Slot[]>(withIds([], store.startupSchedule().slots));
  let schedulePath = store.startupSchedule().path;
  let scheduleError = $state(store.startupSchedule().error);
  function applySchedule(f: api.ScheduleFile) {
    scheduleError = f.error;
    // a file that does not parse keeps the list on screen
    if (!f.error) slots = withIds(slots, f.slots);
  }
  async function setSlots(next: Slot[]) {
    slots = next;
    try {
      applySchedule(await api.saveSchedule(next));
    } catch (e) {
      scheduleError = String(e);
    }
  }
  /** settings.toml changed, by hand or by our own save. */
  function applySettings(f: api.SettingsFile) {
    settingsError = f.error;
    if (f.error || !store.adoptSettings(f.settings)) return;
    const s = f.settings;
    squelch = s.squelch ?? DEFAULT_SQUELCH;
    showDeadAir = s.show_dead_air ?? false;
    ppmByHand = s.crystal_ppm != null;
    if (s.crystal_ppm != null) ppm = s.crystal_ppm;
    if ((s.location ?? '') !== location) setLocation(s.location ?? '');
  }
  let fileError = $derived(settingsError ?? scheduleError);
  let dismissed = $state(new Set<string>());
  // plans that tune by themselves give no reminder, unless another plan already has the radio
  let due = $derived(reminders(slots, new Date(nowS * 1000)).filter((r) => !dismissed.has(r.key) && (r.slot.action === 'remind' || (r.phase === 'now' && !!onPlan && r.key !== onPlan.key))));
  const notified = new Set<string>();
  $effect(() => {
    for (const r of due) {
      const key = r.key + r.phase;
      if (notified.has(key)) continue;
      notified.add(key);
      if (r.slot.action === 'remind') api.notify(`${r.slot.name || 'Time to tune in'} ${r.phase === 'now' ? 'is starting' : when(r.start, new Date())}`, `${fmtMHz(r.slot.mhz * 1e6)} MHz ${MODE_NAMES[r.slot.mode]}`).catch(() => {});
    }
  });
  // A plan that tunes by itself: takes the radio when it starts, gives it back when it ends if asked to.
  let onPlan = $state<(Window & { back: { hz: number; mode: Mode } | null; recording: boolean; toRecord: boolean }) | null>(null);
  const planSeen = new Set<string>();
  function startPlan(w: Window) {
    planSeen.add(w.key);
    const back = w.slot.go_back ? { hz: tuned, mode } : null;
    const hz = Math.round(w.slot.mhz * 1e6);
    goTo(hz, w.slot.mode);
    if (!playing) togglePlay();
    onPlan = { ...w, back, recording: false, toRecord: w.slot.action === 'record' && !rec?.recording };
  }
  function endPlan() {
    if (!onPlan) return;
    const { back, recording, slot } = onPlan;
    onPlan = null;
    if (recording) setRecording(false);
    // moved away by hand: stay where they went
    if (back && Math.abs(tuned - slot.mhz * 1e6) < 1_000) goTo(back.hz, back.mode);
  }
  function goTo(hz: number, m: Mode) {
    const b = bandAt(plan, hz);
    if (b && b.id !== bandId) pickBand(b);
    tuneSaved(hz, m);
    // `send` waits for a frame, and a window nobody can see (lid shut, screen off) never draws one
    cancelAnimationFrame(pending);
    pending = 0;
    api.tune(center, tuned);
  }
  /** Called each second: starts and ends plans, and keeps the wake alarm and the awake hold up to date. */
  let wakeAt: number | null | undefined;
  let holdTo: number | null = null;
  let wakeError = $state<string | null>(null);
  function runPlans() {
    const now = new Date();
    if (onPlan && now >= onPlan.end) endPlan();
    // only once the engine is there, so the file has the plan's frequency in its name and nothing from before
    if (onPlan?.toRecord && status && Math.abs(status.tuned_hz - onPlan.slot.mhz * 1e6) < 1_000) {
      onPlan.toRecord = false;
      onPlan.recording = true;
      setRecording(true);
    }
    if (!onPlan) {
      const w = openWindows(slots, now).find((x) => !planSeen.has(x.key));
      if (w) startPlan(w);
    }
    const wake = nextWake(slots, now);
    const at = wake && Math.round(wake.getTime() / 1000);
    // while it can't, keep trying: the setup command runs in a terminal and nothing tells us when it is done
    if (at !== wakeAt || (wakeError && now.getSeconds() % 5 === 0)) {
      wakeAt = at;
      api.setWake(at).then(
        () => (wakeError = null),
        (e) => (wakeError = String(e)),
      );
    }
    const until = awakeUntil(slots, now)?.getTime() ?? null;
    if (until !== holdTo) {
      holdTo = until;
      api.holdAwake(until ? Math.ceil((until - now.getTime()) / 1000) : 0).catch(() => {});
    }
  }
  function followReminder(r: (typeof due)[number]) {
    dismissed = new Set(dismissed).add(r.key);
    tuneCentered(Math.round(r.slot.mhz * 1e6));
    if (r.slot.mode !== mode) {
      mode = r.slot.mode;
      api.setMode(mode);
    }
  }
  function tuneSaved(hz: number, m: Mode | null | undefined) {
    tuneCentered(hz);
    if (m && m !== mode) {
      mode = m;
      api.setMode(m);
    }
  }
  /** Tunes a bookmark from the Bookmarks screen, entering its band first, and goes back to the waterfall. */
  function tuneBookmark(m: Saved) {
    if (m.band.id !== bandId) pickBand(m.band);
    tuneSaved(Math.round(m.mhz * 1e6), m.mode);
    bookmarksOpen = false;
  }
  /** The top bar's frequency: back to the waterfall with it in the middle. */
  function showTuned() {
    bookmarksOpen = false;
    leaveRecordings();
    tuneCentered(tuned);
  }
  // the one bookmark control: saved under the name this frequency already has, else its number
  let tunedMark = $derived(bookmarkNear(saved, tuned, 1_500));
  function toggleBookmark() {
    if (tunedMark) return deleteBookmark(tunedMark.band.id, tunedMark.mhz);
    const b = bandAt(plan, tuned);
    if (b) saveBookmark(b.id, { mhz: Math.round(tuned / 100) / 1e4, name: (rds && rdsName(rds)) ?? listeningMark?.name ?? tunedChannel?.name ?? fmtMHz(tuned), mode: mode === b.mode ? undefined : mode });
  }
  async function saveBookmark(bandId: string, b: api.Bookmark) {
    try {
      applyBands(await api.saveBookmark(bandId, b));
    } catch (e) {
      bandsError = String(e);
    }
  }
  async function deleteBookmark(bandId: string, mhz: number) {
    try {
      applyBands(await api.deleteBookmark(bandId, mhz));
    } catch (e) {
      bandsError = String(e);
    }
  }
  function applyBands(nb: api.Bands) {
    setRepeaters(repeaters, nb.plan.bands);
    plan = nb.plan;
    bandsPath = nb.path;
    bandsError = nb.error;
  }
  let dial = $derived(band?.mode === 'wfm' ? dialRows(fmNearby.length > 0, fmLocal, heard.fm ?? [], status?.signals ?? [], rdsHeard, startedAt) : []);
  let dialSummary = $derived(`${dial.filter((r) => r.hz >= vs && r.hz <= ve).length} in view · ${dial.length} in all`);
  let panels = $derived.by(() => {
    const own: PanelId[] = [];
    if (band?.id === 'iss') own.push('iss');
    if (decoding) own.push('sondes');
    if (band?.decoder === 'acars') own.push('acars');
    if (band?.decoder === 'adsb') own.push('aircraft');
    if (band?.decoder === 'adsb' || band?.decoder === 'acars') own.push('flights');
    if (p25 && !emsScreen) own.push('p25');
    if (repReachKm !== undefined && (repInBand.length || (mapLoaded && iss?.site))) own.push('repeaters');
    if (band?.mode === 'wfm') own.push('stations');
    // what HD stations send besides audio, once there is some
    if (hdTraffic || hdWeather) own.push('hd');
    if (board) {
      own.push('board');
      // the history is worth a tab once there are a few minutes of it
      if (boardListened >= SHOW_AFTER_MIN) own.push('activity');
    } else if (chList.length) own.push('channels');
    // a pass overhead is worth a tab on any band, but never opens a panel by itself
    if (own.length && issUp && !own.includes('iss')) own.push('iss');
    return own;
  });
  let chList = $derived(channelList(band?.id, status?.signals ?? []));

  // Bands of channels: tiles, and each minute's channels on air for the Activity tab (`<board id>.history` in the
  // state file, last KEEP_MIN minutes, recorded whenever a side of that band is in the capture, whichever band is open).
  let board = $derived(boardFor(band?.id));
  let boardSides = $derived(board ? sidesIn(board, center - rate * 0.45, center + rate * 0.45) : []);
  let boardLive = $derived(board ? liveOn(board, status?.signals ?? []) : new Map<string, number>());
  const boardHist = new Map<string, Minute[]>(BOARDS.map((b) => [b.id, parseHistory(store.getState(`${b.id}.history`, []), Date.now())]));
  // bumped when a minute ends, so the grid redraws once a minute and not per status
  let boardRev = $state(0);
  let boardListened = $derived.by(() => {
    void boardRev;
    return board ? listened(boardHist.get(board.id)!, Date.now()) : 0;
  });
  let boardAct = $derived.by(() => {
    void boardRev;
    return board ? activity(board, boardHist.get(board.id)!, Date.now()) : { spanMin: 0, rows: [] };
  });
  function recordBoards(signals: { hz: number; snr_db?: number }[]) {
    for (const b of BOARDS) {
      const sides = sidesIn(b, center - rate * 0.45, center + rate * 0.45);
      if (!sides.length) continue;
      const hist = boardHist.get(b.id)!;
      if (record(b, hist, Date.now(), sides, liveOn(b, signals).keys())) {
        store.setState(`${b.id}.history`, hist);
        boardRev++;
      }
    }
  }
  let boardSummary = $derived.by(() => {
    const busiest = boardAct.rows.map((r) => ({ key: r.channel.name ?? r.channel.key, sum: r.cells.reduce<number>((a, v) => a + (v ?? 0), 0) })).sort((a, b) => b.sum - a.sum)[0];
    return `Last ${spanText(boardAct.spanMin)}${busiest?.sum ? ` · busiest: ${busiest.key}` : ' · nothing heard'}`;
  });
  let chLive = $derived(chList.reduce((n, s) => n + s.channels.filter((c) => c.live).length, 0));
  let chSummary = $derived(chLive ? `${chLive} on air` : 'Nothing on air right now');
  let panel = $derived(panelPick && panels.includes(panelPick) ? panelPick : (panels[0] ?? null));
  $effect(() => {
    if (panel !== 'flights') return;
    const load = () => api.airFlights(FLIGHT_LOG_DAYS).then((f) => (flightList = f));
    load();
    const t = setInterval(load, FLIGHT_REFRESH_MS);
    return () => clearInterval(t);
  });
  const PANEL_TITLE: Record<Exclude<PanelId, 'board'>, string> = { activity: 'Activity', sondes: 'Weather balloons', acars: 'Aircraft messages', aircraft: 'Aircraft', flights: 'Flights log', stations: 'Stations', hd: 'Traffic and weather', p25: 'EMS', iss: 'Space station', repeaters: 'Repeaters', channels: 'Channels' };
  const panelTitle = (id: PanelId) => (id === 'board' ? (board?.title ?? 'Channels') : PANEL_TITLE[id]);
  let issHue = $derived(hues[plan.bands.find((b) => b.id === 'iss')?.group ?? ''] ?? hue);
  const panelHue = (id: PanelId) => (id === 'iss' ? issHue : hue);
  let panelSummary = $derived(
    panel === 'sondes' ? sondeLine : panel === 'acars' ? acarsSummary(status?.acars ?? null, acarsList.length) : panel === 'aircraft' ? adsbLine : panel === 'flights' ? `${flightList.length} flight${flightList.length === 1 ? '' : 's'} in the last ${FLIGHT_LOG_DAYS} days` : panel === 'stations' ? dialSummary : panel === 'hd' ? (hd?.name ?? '') : panel === 'p25' && p25 ? p25Summary(p25) : panel === 'iss' ? issMapSummary : panel === 'repeaters' ? repSummary : panel === 'channels' || panel === 'board' ? chSummary : panel === 'activity' ? boardSummary : '',
  );

  let wfSummary = $derived(
    `${band?.name ?? 'Tuned'} · ${fmtMHz(vs)} – ${fmtMHz(ve)} MHz` +
      (decoding ? ` · decoding weather balloons on ${status?.decoding_hz.length ?? 0} signal${status?.decoding_hz.length === 1 ? '' : 's'}` : acarsOn ? ' · decoding ACARS' : adsbOn ? ' · decoding aircraft across the capture' : band?.decoder ? ` · the ${band.decoder} decoder is not built yet; this is the raw signal` : ''),
  );

  // which pane has the screen; it lets go when the panel changes
  let maxed = $state<{ pane: 'wf' | 'panel'; panel: PanelId } | null>(null);
  let maxPane = $derived(maxed && maxed.panel === panel ? maxed.pane : null);
  function toggleMax(pane: 'wf' | 'panel') {
    if (!panel) return;
    maxed = maxPane === pane ? null : { pane, panel };
  }

  // the panel's share of the width (or height when stacked), dragged at the divider
  let panelShare = $state(store.getState('panel.size', 30));
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
      store.setState('panel.size', panelShare);
    };
    grip.addEventListener('pointermove', move);
    grip.addEventListener('pointerup', stop);
  }

  // drag, zoom and tune are shown once, until dismissed
  let hintSeen = $state(store.getState('hint.seen', false));
  function dismissHint() {
    hintSeen = true;
    store.setState('hint.seen', true);
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
    // while a recording plays, the play button is its
    if (recOwns && play) return void api.playPause(!play.paused);
    // pausing a tower is leaving it: the map and the messages come back
    if (airListen && playing) return stopListening();
    playing = !playing;
    api.setPlaying(playing);
  }

  /** Back to watching planes; the radio goes quiet, since there is nothing to hear while it takes turns. */
  function stopListening() {
    airListen = null;
    playing = false;
    api.setPlaying(false);
  }

  function toggleRecord() {
    setRecording(!rec?.recording);
  }
  async function setRecording(on: boolean) {
    await api.record(on);
    // the engine starts or ends the file on its next pass
    setTimeout(async () => {
      rec = await api.recording();
      recAt = Date.now() / 1000 - rec.secs;
      if (!on && rec.file) {
        recEndS = Date.now() / 1000;
        recCopied = false;
        loadRecordings();
      }
    }, 250);
  }
  async function loadRecordings() {
    recordingsList = await api.recordings();
  }
  function openRecordings(file: string | null = null) {
    recFocus = file;
    bookmarksOpen = false;
    recordingsOpen = true;
    loadRecordings();
  }
  /** Closes the screen; a recording playing pauses where it is, and the live sound comes back. */
  function leaveRecordings() {
    if (play && !play.paused) api.playPause(true);
    recordingsOpen = false;
  }
  async function removeRecording(file: string) {
    try {
      await api.deleteRecording(file);
    } catch (e) {
      console.error(e);
    }
    loadRecordings();
  }
  /** Stops the recording and tunes the radio where it was made. */
  function hearLive(hz: number, m: Mode) {
    api.playRecording(null);
    const b = bandAt(plan, hz);
    if (b && b.id !== bandId) pickBand(b);
    tuneSaved(hz, m);
    leaveRecordings();
  }
  const recordingName = (hz: number, m: Mode) => bookmarkNear(named, hz, 1_000)?.name ?? (m === 'wfm' ? stationName(hz) : undefined);
  // when one ends, the next older one plays; none left, the live sound comes back
  $effect(() => {
    if (!play?.done || playedOn === play.file) return;
    playedOn = play.file;
    const next = nextAfter(recordingsList, play.file);
    api.playRecording(next ? next.file : null);
  });
  async function copyRecording() {
    if (!rec?.file) return;
    await navigator.clipboard.writeText(rec.file);
    recCopied = true;
  }

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || e.target instanceof HTMLTextAreaElement) return;
    const keyed = railPlan.groups.flatMap((g) => railPlan.bands.filter((b) => b.group === g.id)).slice(0, 9);
    if (/^[1-9]$/.test(e.key) && keyed[+e.key - 1]) pickBand(keyed[+e.key - 1]);
    else if (e.key === 'ArrowRight') step(1);
    else if (e.key === 'ArrowLeft') step(-1);
    else if (e.key === '[') setView(vs - span / 4, ve - span / 4);
    else if (e.key === ']') setView(vs + span / 4, ve + span / 4);
    else if (e.key === '-') zoom(1.5);
    else if (e.key === '=' || e.key === '+') zoom(1 / 1.5);
    else if (e.key === 'f') toggleMax('panel');
    else if (e.key === 'b') {
      leaveRecordings();
      bookmarksOpen = !bookmarksOpen;
    } else if (e.key === 'r') {
      if (recordingsOpen) leaveRecordings();
      else openRecordings();
    }
    else if (e.key === 'F11') toggleFull();
    else if (e.key === 'Escape' && settingsOpen) settingsOpen = false;
    else if (e.key === 'Escape' && bookmarksOpen) bookmarksOpen = false;
    else if (e.key === 'Escape' && recordingsOpen) leaveRecordings();
    else if (e.key === 'Escape' && recOwns) api.playRecording(null);
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
      plan = b.plan;
      bandsPath = b.path;
      bandsError = b.error;
      loadRepeaters();
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
              store.setState('stations', heard);
            }
          }
          persist.update(s.signals.map((d) => d.hz), Date.now());
          if (s.hd_images) hdImages = s.hd_images;
          if (!s.hd) hdImages = [];
          if (s.hd?.digital) hdAudioAt = Date.now();
          recordBoards(s.signals);
          status = s;
          if (s.look?.line && s.look.line.seq !== emsLines[0]?.seq) {
            emsLines = addLine(emsLines, s.look.line);
            emsLineAt = Date.now() / 1000;
          }
        },
      );
      const fm = plan.bands.find((x) => x.id === 'fm') ?? plan.bands[0];
      if (fm) pickBand(fm);
      loadIss();
      unsubs.push(api.onIss(loadIss));
      unsubs.push(api.onTheme(setTheme));
      unsubs.push(
        api.onBands(applyBands),
      );
      unsubs.push(
        api.onSettings(applySettings),
      );
      unsubs.push(
        api.onSchedule(applySchedule),
      );
      unsubs.push(
        api.onRepeaters(loadRepeaters),
      );
    })();
    // Drives the header clocks and keeps the pass list current.
    const tick = setInterval(() => {
      nowS = Date.now() / 1000;
      runPlans();
    }, 1000);
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
    if (squelch !== (store.getSettings().squelch ?? DEFAULT_SQUELCH)) store.setSetting('squelch', squelch);
  });
  $effect(() => {
    api.setGain(gain === 'auto' ? null : +gain);
  });
  $effect(() => {
    api.setPpm(ppm);
    if (ppmByHand) store.setSetting('crystal_ppm', ppm);
    else store.setState('ppm.measured', ppm);
  });
  $effect(() => {
    const m = status?.crystal ? Math.round(status.crystal.ppm) : null;
    if (!ppmByHand && m !== null && Math.abs(m) <= MAX_PPM && m !== ppm) ppm = m;
  });
  function setPpmByHand(v: string) {
    ppmByHand = v.trim() !== '' && Number.isFinite(+v);
    if (!ppmByHand) store.setSetting('crystal_ppm', null);
    if (ppmByHand) ppm = Math.max(-MAX_PPM, Math.min(MAX_PPM, Math.round(+v)));
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app" style:--accent={hue} style:--hue={hue} style:--ramp={ramp}>
  {#if !full}
    <BandRail plan={railPlan} current={bookmarksOpen || recordingsOpen ? null : railId(railPlan, bandId)} {recordingsOpen} recordingCount={recordingsList.length} onRecordings={() => (recordingsOpen ? (leaveRecordings()) : openRecordings())} {bookmarksOpen} bookmarkCount={saved.filter((m) => bookmarkPlan.bands.some((b) => b.id === m.band.id)).length} onBookmarks={() => { leaveRecordings(); bookmarksOpen = !bookmarksOpen; }} {outside} open={railOpen} onPick={pickBand} onToggle={toggleRail} {hues} marks={railMarks} {settingsOpen} onSettings={() => (settingsOpen = !settingsOpen)} {settings} />
  {/if}

  <div class="right">
    <header>
      <div class="freq">
        <button class="digits mono" onclick={showTuned} title={airWatching ? 'Where the radio is now; it takes turns between planes and messages' : 'Show this frequency on the waterfall'}><em>{fmtHz(shownHz).split('.')[0]}</em>.{fmtHz(shownHz).split('.').slice(1).join('.')}</button>
        <span class="unit mono">MHz</span>
        {#if outside}<span class="outside mono">outside {band?.name}</span>{/if}
      </div>
      <div class="now">
        <span class="titleline">
          <span class="title">{nowTitle}</span>
          <button class="keep" class:on={tunedMark} aria-pressed={!!tunedMark} disabled={!tunedMark && !bandAt(plan, tuned)} onclick={toggleBookmark} aria-label={tunedMark ? 'Remove this bookmark' : 'Bookmark this frequency'} title={tunedMark ? 'Remove this bookmark' : 'Bookmark this frequency (b lists them all)'}><Icon name="bookmark" size={14} filled={!!tunedMark} /></button>
        </span>
        <span class="sub copy" title={p25?.scan ? 'The radio hops across the whole band; every carrier that stays on is tested for P25 frames. Click a frequency to stop and tune by hand.' : undefined}>{nowSub}</span>
      </div>
      <button class="play" class:on={barPlay ? !barPlay.paused : playing && !airWatching} disabled={airWatching && !barPlay} onclick={togglePlay} aria-label={(barPlay ? !barPlay.paused : playing) ? 'Pause' : 'Play'} title={barPlay ? `${barPlay.paused ? 'Play' : 'Pause'} the recording (space)` : airWatching ? 'Pick a plane on the map and press barPlay on its card' : `${playing ? 'Pause' : 'Play'} (space)`}>
        <Icon name={(barPlay ? !barPlay.paused : playing && !airWatching) ? 'pause' : 'play'} size={20} />
      </button>
      <button class="rec" class:on={recLit} onclick={toggleRecord} aria-label={rec?.recording ? 'Stop recording' : 'Record'} title={rec?.recording ? 'Stop recording' : 'Record what you hear to a file'}>
        <Icon name="record" size={18} />
      </button>
      {#if rec?.recording}
        <span class="rec-note mono">{Math.floor(Math.max(0, nowS - recAt) / 60)}:{String(Math.floor(Math.max(0, nowS - recAt)) % 60).padStart(2, '0')}</span>
      {:else if rec?.error}
        <span class="rec-note err copy" title={rec.error}>Not recording: {rec.error}</span>
      {:else if rec?.file && nowS - recEndS < SAVED_FOR_S}
        <button class="rec-file" onclick={copyRecording} aria-label="Copy where the recording was saved" title={recCopied ? 'Copied the file\'s path' : 'Copy the file\'s path'}>
          <Icon name="folder" size={16} />
        </button>
        <button class="rec-note mono link" onclick={() => openRecordings(rec!.file)} title="Open the recordings list on this one (R)">{recCopied ? 'Copied' : 'Saved'} {clockLength(rec.secs)}</button>
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
      {#each due as r (r.key)}
        <span class="chip warn remind">
          <button onclick={() => followReminder(r)} title="Tune {fmtMHz(r.slot.mhz * 1e6)} MHz {MODE_NAMES[r.slot.mode]}">{r.slot.name || 'Time to tune in'} · {r.phase === 'now' ? 'starting now' : when(r.start, new Date(nowS * 1000))} · Tune</button>
          <button onclick={() => (dismissed = new Set(dismissed).add(r.key))} aria-label="Dismiss" title="Dismiss">×</button>
        </span>
      {/each}
      {#if status?.simulated}<span class="chip warn" title={status.device}>Simulated</span>{/if}
      {#if status?.error || bandsError || fileError}<button class="chip err" onclick={() => (settingsOpen = true)} title={status?.error ?? bandsError ?? fileError ?? ''}>Problem · details</button>{/if}
      <button class="iconbtn" onclick={toggleFull} title={full ? 'Exit full screen (F11)' : 'Full screen (F11)'} aria-label={full ? 'Exit full screen' : 'Full screen'}>
        <Icon name={full ? 'shrink' : 'expand'} size={18} />
      </button>
      {#if band?.id === 'air' && !bookmarksOpen}
        {#if planesScreen}
          <button class="chip backair" onclick={() => (seeRadio = true)} title="The waterfall and every aircraft frequency">See the radio</button>
        {:else}
          <button class="chip backair" onclick={() => { seeRadio = false; airListen = null; }}>Back to the map</button>
        {/if}
      {/if}
      {#if isEms(band) && !bookmarksOpen}
        {#if emsScreen}
          <button class="chip backair" onclick={() => (seeRadio = true)} title="The waterfall and the whole band">See the radio</button>
        {:else}
          <button class="chip backair" onclick={() => (seeRadio = false)}>Back to the tower</button>
        {/if}
      {/if}
    </header>

    {#if onPlan}
      <div class="planbar">
        <b>{onPlan.slot.name || fmtMHz(onPlan.slot.mhz * 1e6)}</b>
        <span class="mono">{fmtMHz(onPlan.slot.mhz * 1e6)} MHz</span>
        <span class="mono">{Math.floor(Math.max(0, onPlan.end.getTime() / 1000 - nowS) / 60)}:{String(Math.floor(Math.max(0, onPlan.end.getTime() / 1000 - nowS)) % 60).padStart(2, '0')} left</span>
        {#if onPlan.recording}<span>recording</span>{/if}
        {#if onPlan.back}<span class="dim">then back to {fmtMHz(onPlan.back.hz)}</span>{/if}
        <span class="planacts">
          {#if onPlan.back}<button onclick={() => onPlan && (onPlan.back = null)}>Stay here after</button>{/if}
          <button onclick={endPlan}>End now</button>
        </span>
      </div>
    {/if}

    <!-- one waterfall: in the Spectrum pane, or in the Airplanes screen while it listens to a tower -->
    {#snippet wfView()}
      <Waterfall
        bind:this={waterfall}
        vs={airWatching ? airVs : vs}
        ve={airWatching ? airVe : ve}
        {rate}
        {stops}
        {tuned}
        bandwidth={BANDWIDTH[mode]}
        specH={maxPane === 'panel' || airWatching ? STRIP_SPEC_H : fit.spec}
        colors={wfColors}
        onTune={(hz, pxHz) => tuneClick(snapTune(hz, status?.signals ?? [], named, pxHz, mode, band?.id))}
        onView={(a, b) => {
          if (airWatching) return;
          cancelAnimationFrame(slideFrame);
          setView(a, b);
        }}
        bandId={band?.id}
        band={band ? { lo: band.low_mhz * 1e6, hi: band.high_mhz * 1e6, name: band.name } : undefined}
      />
    {/snippet}
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
        <div class="box" class:panning class:strip={maxPane === 'panel'} bind:clientWidth={boxW} bind:clientHeight={boxH}>
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
          {#if emsLook && maxPane !== 'panel'}
            <EmsStrip ranges={emsLook} lines={emsLines} looking={!!status?.look?.looking} agoS={emsLineAt === null ? null : Math.max(0, nowS - emsLineAt)} {vs} {ve} {tuned} scan={p25?.scan ?? null} tried={p25?.tried ?? []} palette={stripPalette} {hue} onTune={tuneCentered} />
          {/if}
          {#if maxPane !== 'panel'}
            <div class="lane l{fit.level}" style:height="{fit.lane}px">
              {#each dots as d}
                <button class="dot {d.cls}" style:left="{d.x}px" title={d.title} aria-label={d.title} onclick={() => tuneSnapped(d.hz)}></button>
              {/each}
              {#each fit.level < LANES.length - 1 ? cards : [] as c (c.at)}
                <span class="stem" class:sel={c.selected} style:left="{c.x}px"></span>
                <button class="card" class:sel={c.selected} class:hdon={c.selected && hdProgram !== null} class:quiet={c.item.quiet} class:typed={c.type} style:left="{cardLeft(c.x, c.w)}px" style:width="{c.w}px" title="{fmtMHz(c.at)} MHz · {c.item.title} · {c.item.sub}" onclick={() => tuneSnapped(c.item.hz)}>
                  <span class="line">
                    {#if c.mark}<span class="kept"><Icon name="bookmark" size={10} filled /></span>{/if}
                    <span class="ttl">{c.item.title}</span>
                    {#if c.selected && mode === 'wfm' && (hd || status?.hd_sidebands || hdProgram !== null)}
                        <span class="hdchip" class:on={hdProgram !== null} role="switch" aria-checked={hdProgram !== null} tabindex="0" title="This station also broadcasts digitally (HD Radio). Click to cycle through its programs and back to analog." onclick={toggleHd} onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && toggleHd(e)}>{hdProgram === null ? 'HD' : hdLabel(hdProgram)}</span>
                      {/if}
                    {#if !c.item.quiet}<span class="bar"><Level pct={c.meter} width="14px" height="3px" /></span>{/if}
                  </span>
                  {#if c.type}<span class="kind">{c.type}</span>{/if}
                </button>
              {/each}
            </div>
          {/if}
          <div class="wf">
            {#if !planesScreen && !emsScreen}{@render wfView()}{/if}
          </div>
          {#if maxPane !== 'panel'}
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
        <Pane title={panelTitle(panel)} summary={panelSummary} accent={panelHue(panel)} maxed={maxPane === 'panel'} mini={maxPane === 'wf'} onMax={() => toggleMax(maxPane === 'wf' ? 'wf' : 'panel')}>
          {#snippet tabs()}
            {#each panels.filter((p) => p !== panel) as p (p)}
              <button class="ptab" style:--c={panelHue(p)} onclick={() => (panelPick = p)} title="Also relevant now: show {panelTitle(p)}">
                {panelTitle(p)}{#if p === 'iss' && issUp}<b> • now</b>{/if}
              </button>
            {/each}
            {#if panel === 'repeaters'}
              <button class="maploc" onclick={changeLocation} title="Change your location in Settings">
                <Icon name="pin" size={12} /><span>{location || 'Set location'}</span>
              </button>
            {/if}
          {/snippet}
          {#if panel === 'sondes'}
            <div class="pbody wrap">
              {#each sondes as o (o.hz)}
                {@const drift = sondeDrift(o)}
                <button class="item balloon" class:sel={Math.abs(o.hz - tuned) < 10_000} onclick={() => tuneClick(o.hz)}>
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
          {:else if panel === 'acars'}
            <div class="pbody acars">
              {#if acarsList.length}<input class="in airfind" bind:value={acarsQuery} placeholder="Find a flight, aircraft or word" aria-label="Find a message" />{/if}
              {#each acarsShown as m (m.at_ms + ':' + m.id)}
                {@const at = acarsPosition(m)}
                <div class="item msg copy">
                  <span class="mhead">
                    <span class="name">{acarsWho(m)}</span>
                    {#if m.flight && m.registration}<span class="dim">{m.registration}</span>{/if}
                    <span class="dim" title="Message label {m.label}">{labelName(m.label) || m.label}</span>
                    <span class="mono dim when">{acarsDay(m.at_ms)}{acarsTime(m.at_ms)} · {fmtMHz(m.hz)} MHz</span>
                  </span>
                  {#if at}<span class="mono dim">{at.lat.toFixed(3)}, {at.lon.toFixed(3)}</span>{/if}
                  {#if m.text}<span class="mono mtext">{m.text}{m.more ? ' …' : ''}</span>{/if}
                </div>
              {:else}
                <p class="empty">{#if acarsList.length}Nothing matches.{:else}None decoded yet. Every US ACARS channel in the capture is decoded all the time; aircraft on the ground or climbing out nearby send the most.{/if}</p>
              {/each}
            </div>
          {:else if panel === 'aircraft'}
            <div class="split">
              {#if planeMap}
                <div class="mapbox repmap">
                  <AircraftMap planes={planeMap.planes} centre={planeMap.centre} halfKm={planeMap.halfKm} here={!!iss?.site} trails={trailPaths} selected={planePick} onpick={pickPlane} />
                  <span class="maplabel">{iss?.site ? 'Arrows point along the track; faded: no position for 30 s' : 'Centred on the aircraft; set your location to see distances'}</span>
                </div>
              {/if}
              <div class="list" bind:this={planeList}>
                {#each pickedFirst(planeRows, planePick) as r (r.a.icao)}
                  <button class="reprow" class:sel={planePick === r.a.icao} class:alarm={isAlarm(r.a)} onclick={() => pickPlane(r.a.icao)}>
                    <span class="name">{r.name}</span>
                    <span class="dim town">{r.a.emergency ?? aircraftLine(r.a)}</span>
                    {#if r.km !== null}<span class="mono">{Math.round(r.km)} km</span>{/if}
                    <span class="mono dim">{ago(r.a.age_s)}</span>
                  </button>
                  {#if planePick === r.a.icao}
                    <dl class="facts copy">
                      {#each aircraftFacts(r.a) as f (f.label)}
                        <dt>{f.label}</dt>
                        <dd class:mono={f.label === 'Address' || f.label === 'Squawk' || f.label === 'Position'}>{f.value}</dd>
                      {/each}
                    </dl>
                    {#each pickedMessages as m (m.at_ms + ':' + m.id)}
                      <div class="item msg copy">
                        <span class="mhead"><span class="dim">{labelName(m.label) || m.label}</span><span class="mono dim when">{acarsDay(m.at_ms)}{acarsTime(m.at_ms)}</span></span>
                        {#if m.text}<span class="mono mtext">{m.text}</span>{/if}
                      </div>
                    {/each}
                  {/if}
                {:else}
                  <p class="empty">None heard yet. Aircraft broadcast every second or so; a line of sight to the sky matters most, and an antenna outdoors reaches a few hundred kilometres.</p>
                  {#if !iss?.site}<p class="empty">Set your location in Settings (S) to see how far each one is.</p>{/if}
                {/each}
              </div>
            </div>
          {:else if panel === 'flights'}
            <div class="split">
              {#if flightMap}
                <div class="mapbox repmap">
                  <AircraftMap planes={[]} centre={flightMap.centre} halfKm={flightMap.halfKm} here={!!iss?.site} trails={flightPath} selected={flightPick?.icao ?? null} />
                  <span class="maplabel">{flightPick ? flightName(flightPick) : ''}: where it was heard</span>
                </div>
              {/if}
              <div class="list">
                {#if flightList.length}<input class="in airfind" bind:value={flightQuery} placeholder="Find a flight, aircraft, type or country" aria-label="Find a flight" />{/if}
                {#each flightShown as d (d.day)}
                  <p class="airday">{d.day}</p>
                  {#each d.flights as f (f.icao + f.first_ms)}
                    <button class="reprow" class:sel={flightPick?.icao === f.icao && flightPick?.first_ms === f.first_ms} class:alarm={!!f.emergency} onclick={() => pickFlight(f)} title="Address {f.icao}{f.country ? ` · ${f.country}` : ''}{f.squawk ? ` · squawk ${f.squawk}` : ''}">
                      <span class="name">{flightName(f)}</span>
                      <span class="dim town">{flightLine(f)}</span>
                      <span class="mono dim">{acarsTime(f.first_ms).slice(0, 5)}–{acarsTime(f.last_ms).slice(0, 5)}</span>
                    </button>
                  {/each}
                {:else}
                  <p class="empty">{flightList.length ? 'Nothing matches.' : 'Nothing logged yet. Every aircraft heard stays here, with its track, across runs.'}</p>
                {/each}
              </div>
            </div>
          {:else if panel === 'stations'}
            <div class="stationsbody">
              {#if hd}
                <div class="hdnow">
                  {#if hdPicture}
                    <img class="hdart" src={hdPicture.data_url} alt={hdPicture.kind === 'art' ? 'Album art' : 'Station logo'} />
                  {:else if hd.incoming !== null}
                    <div class="hdart hdwait" title="A picture is arriving"><Icon name="image" size={20} /></div>
                  {/if}
                  <div class="hdtext">
                    {#if hd.title}<span class="big">{hd.title}</span>{:else if hd.name}<span class="name">{hd.name}</span>{/if}
                    {#if hd.artist || hd.album}<span class="dim">{[hd.artist, hd.album].filter(Boolean).join(' · ')}</span>{:else if hd.slogan}<span class="dim">{hd.slogan}</span>{/if}
                    {#if hd.error}<span class="dim">{hd.error}</span>{/if}
                    {#if hd.message}<span class="dim">{hd.message}</span>{/if}
                    {#if hd.incoming !== null}<div class="hdload" title="Picture {Math.round(hd.incoming * 100)}% here"><span style:width="{hd.incoming * 100}%"></span></div>{/if}
                  </div>
                  <div class="hdside">
                    {#if !hd.error}
                      <span class="hdstat" title={hdState({ ...hd, digital: hdAudio }, hdProgram !== null)}>
                        <span class="step" class:on={hd.synced} class:seek={!hd.synced}><Icon name="tower" size={16} /></span>
                        <span class="step" class:on={hdAudio} class:live={hdAudio && hdProgram !== null}><Icon name="speaker" size={16} /></span>
                        <Level pct={hd.synced && hd.mer_db !== null ? ((hd.mer_db - HD_MER_DB[0]) / (HD_MER_DB[1] - HD_MER_DB[0])) * 100 : 0} width="70px" height="8px" />
                      </span>
                    {/if}
                    {#if hd.synced}
                      <div class="hdprogs">
                        <button class:sel={hdProgram === null} onclick={() => setHdProgram(null)} title="Analog FM">FM</button>
                        {#each hd.programs.length ? hd.programs : [0] as p}
                          <button class:sel={p === hdProgram} onclick={() => setHdProgram(p)} title="Digital program {p + 1}">{hdLabel(p)}</button>
                        {/each}
                      </div>
                    {/if}
                  </div>
                </div>
              {/if}
              <StationList rows={dial} {hue} {tuned} {vs} {ve} meterDb={METER_DB} onTune={tuneCentered} />
              {#if !fmNearby.length}
                <button class="dialhint" onclick={changeLocation}>Set your location to list every station near you</button>
              {/if}
            </div>
          {:else if panel === 'hd'}
            <div class="pbody hdbody">
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
                <Talkgroups {p25} {picks} rows={tgRows} {fold} onHold={holdGroup} onSkip={skipGroup} />
              </div>
            </div>
          {:else if panel === 'iss'}
            <div class="split over">
              <div class="mapbox issmap">
                <IssMap track={issTrack} now={nowS} site={iss?.site ?? null} local={issLocal} pass={issHover} />
                {#if issHover}<span class="mapcap">{issHover === issUp ? 'Overhead now' : `${dayLabel(issHover.rise, nowS)} ${clock(issHover.rise)}`} · {Math.round(issHover.max_elevation)}° high</span>{/if}
                <span class="maplabel" class:warn={freshness(issAge) === 'aging'} class:err={freshness(issAge) === 'stale' || freshness(issAge) === 'none'}>{ageLabel(issAge)}</span>
              </div>
              <div class="list passes">
                {#if iss?.bad_location}<p class="empty err">Your location in Settings is not a US ZIP code, state or “lat, lon”.</p>{/if}
                {#if iss?.approximate}<p class="empty">Times are for the middle of the state: within a minute nearby, a few minutes off in a far corner of a big state. A ZIP code is closer.</p>{/if}
                <p class="empty" class:warn={freshness(issAge) === 'aging'} class:err={freshness(issAge) === 'stale'}>{issNote || `${ageLabel(issAge)}.`}</p>
                {#if !location}
                  <p class="empty">Set your location in Settings (S) to see when the station passes over.</p>
                {:else}
                  {#each issNext as p (p.rise)}
                    {@const now = p === issUp}
                    {@const colour = passColour(p.max_elevation)}
                    <div class="item pass" class:sel={now} role="button" tabindex="0" title={`${passWhy(p.max_elevation)} ${Math.round((p.set - p.rise) / 60)} min above the horizon, rises ${compass(p.rise_az)}, sets ${compass(p.set_az)}.`} onmouseenter={() => (issHover = p)} onmouseleave={() => (issHover = null)} onfocus={() => (issHover = p)} onblur={() => (issHover = null)}>
                      <span class="name">{now ? 'Overhead now' : `${dayLabel(p.rise, nowS)} ${clock(p.rise)}`} <small>– {clock(p.set)}</small></span>
                      <span class="big" style:color={colour}>{Math.round(p.max_elevation)}° high</span>
                    </div>
                  {/each}
                  <p class="empty">Passes below about 10° are hard to hear. Voice and pictures are on 145.800 MHz, APRS on 145.825 MHz; the station transmits only some of the time.</p>
                {/if}
              </div>
            </div>
          {:else if panel === 'board' && board}
            <ChannelBoard {board} live={boardLive} {tuned} sides={boardSides} fullDb={METER_DB} onTune={tuneClick} />
          {:else if panel === 'activity' && board}
            <BoardActivity {board} act={boardAct} tunedKey={channelOn(board, tuned)?.key} now={nowS * 1000} onTune={tuneClick} />
          {:else if panel === 'channels'}
            <div class="list chlist">
              {#each chList as sec, i (i)}
                {#if sec.group}<h3 class="chgroup">{sec.group}</h3>{/if}
                {#each sec.channels as { channel: c, live, snr } (c.hz)}
                  <button class="reprow chrow" class:sel={c.hz === tunedChannel?.hz} class:live onclick={() => tuneClick(c.hz)} title={live ? `On air${snr !== null ? `, ${snr.toFixed(0)} dB above the noise` : ''}` : undefined}>
                    <span class="dot"></span>
                    {#if bookmarkNear(saved, c.hz, 1_000)}<span class="kept"><Icon name="bookmark" size={10} filled /></span>{/if}
                    <span class="name town">{bookmarkNear(named, c.hz, 1_000)?.name ?? c.name}</span>
                    {#if snr !== null}<Level pct={(snr / METER_DB) * 100} />{/if}
                    <span class="mono">{(c.hz / 1e6).toFixed(4)}</span>
                  </button>
                {/each}
              {/each}
            </div>
          {:else if panel === 'repeaters'}
            {#if iss?.site && !repInBand.length}
              <p class="empty pbody">No repeaters on this band near your location in ~/.config/squelch/repeaters.toml.</p>
            {:else if iss?.site}
              <div class="split">
                <div class="mapbox repmap">
                  <RepeaterMap points={repPoints} reachKm={repReachKm ?? 0} tunedId={repTuned?.id ?? null} live={repLive} onTune={(hz) => tuneSaved(hz, 'nfm')} view={repView} onView={setRepView} site={iss.site} palette={mapPalette} />
                  {#if repTuned}
                    <span class="maplabel">{repTuned.call} · {channelAt(band?.id, repTuned.hz)?.group ?? repTuned.city} · {Math.round(repTuned.km)} km{repTuned.digital ? ` · may be ${repTuned.digital} only` : ''}</span>
                  {/if}
                </div>
              </div>
            {:else}
              <p class="empty pbody">Set your location in Settings (S) to see which repeaters are near you.</p>
            {/if}
          {/if}
        </Pane>
      {/if}
      {#if planesScreen && !bookmarksOpen && !recordingsOpen}
        <AirplanesScreen waterfall={wfView} live={adsb?.aircraft ?? []} messages={joinBlocks(acarsList)} {trails} site={iss?.site ?? null} listening={airListen} {playing} recording={!!rec?.recording} recLit={recLit} onRecord={toggleRecord} palette={mapPalette} onListen={listenTo} onStopListening={stopListening} onSettings={() => (settingsOpen = true)} />
      {/if}
      {#if emsScreen && !bookmarksOpen && !recordingsOpen}
        <EmsScreen {p25} {picks} {lockSecs} rows={tgRows} {fold} otherBand={otherEmsBand?.name ?? null} onHold={holdGroup} onSkip={skipGroup} onOtherBand={goOtherEmsBand} onSeeRadio={() => (seeRadio = true)} lines={emsLines} {named} analog={emsAnalog} onListen={listenPlain} onTowers={backToTowers} />
      {/if}
      {#if recordingsOpen}
        <RecordingsScreen recordings={recordingsList} playing={play} now={nowS} focus={recFocus} {hues} nameOf={recordingName} bandOf={(hz) => bandAt(plan, hz)} onPlay={(f) => api.playRecording(f)} onPause={(p) => api.playPause(p)} onSeek={(s) => api.playSeek(s)} onDelete={removeRecording} onLive={hearLive} onClose={() => (leaveRecordings())} />
      {/if}
      {#if bookmarksOpen}
        <BookmarksScreen plan={bookmarkPlan} {saved} {hues} {tuned} {slots} now={nowS} {wakeError} onTune={tuneBookmark} onTuneAt={(t) => tuneSaved(Math.round(t.mhz * 1e6), t.mode)} onBand={pickBand} onSave={saveBookmark} onDelete={deleteBookmark} onSlots={setSlots} onClose={() => (bookmarksOpen = false)} />
      {/if}
    </main>
  </div>
</div>

{#snippet settings()}
  <label class="field">
    <span class="lbl">Location</span>
    <input class="loc" bind:this={locationInput} value={location} placeholder="80302, Colorado, or 39.74, -104.99" class:bad={iss?.bad_location} onchange={(e) => setLocation(e.currentTarget.value)} />
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
    <span>Show quiet bands<small>Data and tracking, MURS, railroad, military aircraft and ham except 2 m and 70 cm: bands that have given us nothing so far</small></span>
    <input type="checkbox" checked={showDeadAir} onchange={toggleDeadAir} />
  </label>
  {#if status?.error || bandsError || fileError}
    <div class="field">
      <span class="lbl">Problems</span>
      {#if status?.error}<small class="err copy">{status.error}</small>{/if}
      {#if bandsError}<small class="err copy">Bands file: {bandsError}</small>{/if}
      {#if settingsError}<small class="err copy">{settingsError}</small>{/if}
      {#if scheduleError}<small class="err copy">{scheduleError}</small>{/if}
    </div>
  {/if}
  <div class="field">
    <span class="lbl">Receiver</span>
    <small class:warn={status?.simulated}>{status?.device ?? 'Starting…'}</small>
    <small class="copy" title={bandsPath ?? ''}>Bands from {bandsPath?.replace(/^\/home\/[^/]+/, '~') ?? 'built-in list'}</small>
    {#if schedulePath}<small class="copy" title={schedulePath}>Times to tune in in {schedulePath.replace(/^\/home\/[^/]+/, '~')}</small>{/if}
    {#if settingsPath}<small class="copy" title={settingsPath}>Settings in {settingsPath.replace(/^\/home\/[^/]+/, '~')}</small>{/if}
    <small>Colours: {theme.name ?? 'Omarchy theme'}</small>
  </div>
  <div class="field keys mono">
    <span class="lbl">Keys</span>
    <small>1–9 band · ← → next signal</small>
    <small>[ ] move · - + zoom · space listen</small>
    <small>f panel takes the screen · Esc back</small>
    <small>b every saved frequency, times to tune in</small>
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
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
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
  .titleline {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .keep {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--fg-dim);
    cursor: pointer;
  }
  .keep:hover {
    color: var(--fg-bright);
    background: var(--muted);
  }
  .keep.on {
    color: var(--orange);
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
  .rec-note.link {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    color: var(--accent);
    text-decoration: underline dotted;
    cursor: pointer;
  }
  .rec-file {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--fg-dim);
    cursor: pointer;
  }
  .rec-file:hover {
    color: var(--fg);
    background: var(--bg-darker);
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
  .chip.backair {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    cursor: pointer;
  }
  .chip.warn {
    color: var(--yellow);
    background: color-mix(in srgb, var(--yellow) 16%, transparent);
  }
  .chip.err {
    color: var(--red);
    background: color-mix(in srgb, var(--red) 16%, transparent);
  }
  .chip.remind {
    display: flex;
    gap: 2px;
    padding: 0;
  }
  .chip.remind button {
    padding: 2px 8px;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
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
  .planbar {
    flex: none;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 10px 10px 0;
    padding: 6px 12px;
    border-radius: 8px;
    border: 1px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, var(--bg-light));
    font-size: 13px;
  }
  .planbar b {
    color: var(--fg-bright);
  }
  .planbar .dim {
    color: var(--fg-dim);
  }
  .planacts {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }
  .planacts button {
    min-height: 24px;
    padding: 1px 10px;
    border-radius: 6px;
    border: 1px solid var(--muted);
    background: transparent;
    color: var(--fg);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  main {
    position: relative;
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
  .maploc {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    max-width: 130px;
    padding: 1px 4px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--fg-dim);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .maploc span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .maploc:hover,
  .maploc:focus-visible {
    color: var(--fg-bright);
    background: var(--bg-light);
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
  .pbody.acars {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .item.msg {
    flex: none;
  }
  .mhead {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
  }
  .mhead .when {
    margin-left: auto;
  }
  .mtext {
    font-size: 12px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
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
  .item.quiet {
    opacity: 0.55;
    padding: 6px 12px;
  }
  .item .gl {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .item .alarm {
    color: var(--red);
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
  /* Pass cards: short, so every pass in the next two days fits without scrolling. The colour says how well it will be heard. */
  .item.pass {
    cursor: default;
    outline: none;
    padding: 7px 10px;
    gap: 1px;
  }
  .item.pass:hover,
  .item.pass:focus-visible {
    border-color: var(--fg-dim);
  }
  .item.pass .name small {
    font-size: 12px;
    font-weight: 400;
    color: var(--fg-dim);
  }
  .over .list.passes {
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
  }
  .mapcap {
    position: absolute;
    right: 8px;
    top: 6px;
    font-size: 11px;
    color: var(--fg-bright);
    background: color-mix(in srgb, var(--bg-darker) 80%, transparent);
    padding: 2px 6px;
    border-radius: 3px;
    pointer-events: none;
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
    bottom: 2px;
    width: 6px;
    height: 6px;
    margin-left: -3px;
    padding: 0;
    border: 0;
    border-radius: 3px;
  }
  /* a 6 px dot is too small to hit */
  .dot::after {
    content: '';
    position: absolute;
    inset: -6px;
  }
  .dot:hover {
    outline: 1px solid var(--fg-bright);
    outline-offset: 2px;
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
    top: 25px;
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
    top: 2px;
    height: 23px;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    border-radius: 5px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
    text-align: left;
    overflow: hidden;
    transition: left 150ms;
  }
  .line {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .card.typed {
    height: 37px;
    flex-direction: column;
    align-items: stretch;
    gap: 0;
    padding-bottom: 3px;
  }
  .kind {
    font-size: 11px;
    color: var(--fg-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .kept {
    flex: none;
    display: grid;
    place-items: center;
    color: var(--orange);
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
  .ttl {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--fg-bright);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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
  .stationsbody {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .hdnow {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--muted);
  }
  .hdart {
    flex: none;
    width: 52px;
    height: 52px;
    border-radius: 6px;
    object-fit: cover;
  }
  .hdtext {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hdtext span {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hdtext .big {
    font-size: 15px;
    font-weight: 600;
    color: var(--hue);
  }
  .hdtext .dim {
    font-size: 12px;
    color: var(--fg-dim);
  }
  .hdside {
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
  }
  .dialhint {
    flex: none;
    padding: 8px 12px;
    border: 0;
    border-top: 1px solid var(--muted);
    border-radius: 0;
    background: none;
    color: var(--fg-dim);
    font: inherit;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
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
    flex: none;
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
  .reprow.alarm .name {
    color: var(--red);
  }
  .facts {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 12px;
    margin: 2px 8px 8px;
    font-size: 12px;
  }
  .facts dt {
    color: var(--fg-dim);
  }
  .facts dd {
    margin: 0;
  }
  .airfind {
    flex: none;
    box-sizing: border-box;
    width: calc(100% - 16px);
    min-height: 28px;
    margin: 4px 8px;
    padding: 0 8px;
    border: 1px solid var(--muted);
    border-radius: 6px;
    background: var(--bg-light);
    color: var(--fg);
    font: inherit;
  }
  .airday {
    margin: 8px 8px 2px;
    font-size: 11px;
    color: var(--fg-dim);
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
