<script lang="ts">
  // Everything about aircraft, for someone who has never heard of a band: a map, and each plane with what is known
  // about it from every source. Covers `main`; App owns the radio and tells this screen whether it is listening.
  import AircraftMap from './AircraftMap.svelte';
  import * as api from '../lib/api';
  import type { AcarsMessage, Aircraft, Airport, Flight, PlaneInfo, TrackPoint } from '../lib/api';
  import { acarsTime, filterMessages } from '../lib/acars';
  import { HEIGHT_BANDS, aircraftFacts, aircraftRows, filterFlights, flightDays, flightLine, flightName, isAlarm, mapFrame, pickedFirst, planeKind, tracksToDraw, trailPiecesKept, type Trails } from '../lib/adsb';
  import { airportLabel, airportsFor, friendlyCallsign, isFrom, lookupLinks, messageReading, messageSender, messageTitle, planeName, positionsIn, routeOf, routeText, talkGuess, type Talk } from '../lib/airplanes';
  import { toKm, type Palette, type Site } from '../lib/terrain';

  interface Props {
    live: Aircraft[];
    messages: AcarsMessage[];
    trails: Trails;
    site: Site | null;
    listening: Talk | null;
    simulated: boolean;
    problem: string | null;
    volume: number;
    palette: Palette;
    onListen: (t: Talk) => void;
    onStopListening: () => void;
    onSeeRadio: () => void;
    onSettings: () => void;
  }
  let { live, messages, trails, site, listening, simulated, problem, volume = $bindable(), palette, onListen, onStopListening, onSeeRadio, onSettings }: Props = $props();

  const FLIGHT_DAYS = 60;
  const FLIGHTS_REFRESH_MS = 60_000;
  const MY_MESSAGES = 8;
  const MPH_PER_KT = 1.15078;

  let tab = $state<'now' | 'messages' | 'earlier'>('now');
  let pickId = $state<string | null>(null);
  let list_el = $state<HTMLElement | undefined>();
  let pickedMessage = $state<number | null>(null);
  // while listening the map is what it was when the radio left: nothing here is passed off as live
  let frozen = $state<Aircraft[]>([]);
  $effect(() => {
    if (!listening) frozen = live;
  });
  let list = $derived(listening ? frozen : live);
  let rows = $derived(aircraftRows(list, site));
  let picked = $derived(rows.find((r) => r.a.icao === pickId) ?? null);

  // ── airports ────────────────────────────────────
  let airports = $state<Airport[]>([]);
  let airportsAt = '';
  $effect(() => {
    const first = list.find((a) => a.lat !== null);
    const at = site ?? (first ? { lat: first.lat!, lon: first.lon! } : null);
    if (!at) return;
    const key = `${at.lat.toFixed(0)},${at.lon.toFixed(0)}`;
    if (key === airportsAt) return;
    airportsAt = key;
    api.airportsNear(at.lat, at.lon).then((a) => (airports = a));
  });

  // ── the picked plane, from each source ──────────
  let itsMessages = $derived(picked ? messages.filter((m) => isFrom(m, picked!.a)).slice(0, MY_MESSAGES) : []);
  let route = $derived(routeOf(itsMessages));
  let routeAirports = $state<Airport[]>([]);
  let knownAirports = $derived([...airports, ...routeAirports]);
  $effect(() => {
    if (route) api.airportsByCode([route.from, route.to]).then((a) => (routeAirports = a));
  });
  let talk = $derived(picked ? talkGuess(picked.a, airports, route?.to ?? null) : null);
  let pinned = $derived(itsMessages.find((m) => m.id === pickedMessage && positionsIn(m).length));
  let pins = $derived(pinned ? positionsIn(pinned) : []);
  function pickPlane(icao: string) {
    pickId = pickId === icao ? null : icao;
    list_el?.scrollTo(0, 0);
    pickedMessage = null;
    flight = null;
    track = [];
    tab = 'now';
  }
  function trend(a: Aircraft): string {
    if (a.on_ground) return 'on the ground';
    const v = a.vrate_fpm ?? 0;
    return v > 200 ? 'climbing' : v < -200 ? 'coming down' : 'level';
  }
  const height = (a: Aircraft) => (a.alt_ft === null ? '' : `${a.alt_ft.toLocaleString('en-US')} ft`);

  // ── messages, each leading to its plane ─────────
  let query = $state('');
  let shownMessages = $derived(filterMessages(messages.filter((m) => m.text), query));
  let openMessage = $state<number | null>(null);
  const planeOf = (m: AcarsMessage) => list.find((a) => a.lat !== null && isFrom(m, a));
  function pickRow(m: AcarsMessage, key: number) {
    const p = planeOf(m);
    if (p) {
      pickId = p.icao;
      pickedMessage = m.id;
      flight = null;
      track = [];
      tab = 'now';
    } else openMessage = openMessage === key ? null : key;
  }

  // ── earlier flights ─────────────────────────────
  let flights = $state<Flight[]>([]);
  let flightQuery = $state('');
  let flight = $state<Flight | null>(null);
  let track = $state<TrackPoint[]>([]);
  let shownFlights = $derived(flightDays(filterFlights(flights, flightQuery), Date.now()));
  $effect(() => {
    if (tab !== 'earlier') return;
    const load = () => api.airFlights(FLIGHT_DAYS).then((f) => (flights = f));
    load();
    const t = setInterval(load, FLIGHTS_REFRESH_MS);
    return () => clearInterval(t);
  });
  function pickFlight(f: Flight) {
    const same = flight?.icao === f.icao && flight.first_ms === f.first_ms;
    flight = same ? null : f;
    track = [];
    pickId = null;
    if (!same) api.airTrack(f.icao, f.first_ms, f.last_ms).then((t) => (track = t));
  }
  function goTab(t: typeof tab) {
    tab = t;
    if (t !== 'earlier') {
      flight = null;
      track = [];
    }
    if (t !== 'now') pickedMessage = null;
  }

  // ── the map ─────────────────────────────────────
  let pinPoints = $derived<TrackPoint[]>(pins.map((p) => ({ la: p.lat, lo: p.lon, t: 0 })));
  let frame = $derived(mapFrame(flight ? [] : list, site, [...track, ...pinPoints]));
  let mapPlanes = $derived(frame ? frame.planes.map((p) => ({ ...p, name: shortName(list.find((a) => a.icao === p.icao)) ?? p.name })) : []);
  function shortName(a: Aircraft | undefined): string | undefined {
    return a?.callsign ? friendlyCallsign(a.callsign) : undefined;
  }
  let mapTracks = $derived.by(() => {
    if (!frame) return {};
    const c = frame.centre;
    const now = Date.now();
    if (flight) return { [flight.icao]: trailPiecesKept(track, c, now, Infinity) };
    return tracksToDraw(trails, frame.planes, selected, c, now);
  });
  // a public database's answer for the plane opened, asked once per opening
  let pickedIcao = $derived(picked?.a.icao ?? null);
  let info = $state.raw<PlaneInfo | null>(null);
  $effect(() => {
    const icao = pickedIcao;
    info = null;
    if (!icao) return;
    let stale = false;
    api.planeInfo(icao).then((i) => {
      if (!stale) info = i;
    });
    return () => (stale = true);
  });
  let mapAirports = $derived.by(() => {
    if (!frame) return [];
    const c = frame.centre;
    const reach = frame.halfKm * 2;
    return airportsFor(airports, frame.halfKm)
      .map((a) => {
        const [x, y] = toKm(c, a.lon, a.lat);
        return { id: a.ident, label: airportLabel(a), name: a.name, x, y, big: a.size === 'L', hit: talk?.airport.ident === a.ident };
      })
      .filter((a) => Math.abs(a.x) < reach && Math.abs(a.y) < reach);
  });
  let mapPins = $derived(frame ? pins.map((p) => { const [x, y] = toKm(frame!.centre, p.lon, p.lat); return { x, y }; }) : []);
  let mapLink = $derived.by(() => {
    const p = picked?.a;
    if (!frame || !talk || p?.lat === null || p?.lon === null || !p) return null;
    const [x1, y1] = toKm(frame.centre, p.lon!, p.lat!);
    const [x2, y2] = toKm(frame.centre, talk.airport.lon, talk.airport.lat);
    return { x1, y1, x2, y2 };
  });
  let selected = $derived(flight?.icao ?? pickId);
  let overhead = $derived(list.length === 1 ? '1 overhead' : `${list.length} overhead`);
</script>

{#snippet reading(m: AcarsMessage)}
  {@const r = messageReading(m, knownAirports)}
  <span class="read">
    {#each r.facts as f (f.label)}<span class="fact"><span class="dim">{f.label}</span> {f.value}</span>{/each}
    {#if r.note}<span class="dim note">{r.note}</span>{/if}
    {#if m.text}<pre class="copy">{m.text}</pre>{/if}
  </span>
{/snippet}

<div class="air">
  <div class="status">
    {#if listening}
      <span class="pill listen">Listening to {listening.name}</span>
      <span class="dim">Map paused</span>
      <label class="vol"><span class="dim">Volume</span><input type="range" min="0" max="1.5" step="0.05" bind:value={volume} /></label>
      <span class="grow"></span>
      <button class="btn main" onclick={onStopListening}>Back to watching planes</button>
    {:else}
      <b class="ttl">Airplanes</b>
      <span class="pill">Watching · {overhead}</span>
      {#if simulated}<span class="chip warn">Simulated: no radio found</span>{/if}
      {#if problem}<button class="chip err" onclick={onSettings}>{problem}</button>{/if}
      <span class="grow"></span>
      <button class="btn" onclick={onSeeRadio} title="Shows the radio's waterfall and every aircraft frequency">See the radio</button>
    {/if}
  </div>
  <div class="body">
    <div class="map" class:paused={!!listening}>
      {#if frame}
        <AircraftMap planes={mapPlanes} centre={frame.centre} halfKm={frame.halfKm} here={!!site} tracks={mapTracks} {selected} onpick={pickPlane} airports={mapAirports} link={mapLink} pins={mapPins} {palette} />
      {:else}
        <p class="empty">Waiting for the first plane. Planes appear here once the radio hears one.</p>
      {/if}
      <div class="key">
        {#if frame}
          <span class="heights">Height, ft
            {#each HEIGHT_BANDS as b (b.label)}<i style="background:{b.color}"></i>{b.label}{/each}
          </span>
        {/if}
        {#if talk}<span><i class="dash"></i>Probably talking to {talk.name}</span>{/if}
        {#if pins.length}<span class="pinkey">● Where its message said it was</span>{/if}
        {#if flight}<span>Path of {friendlyCallsign(flightName(flight))}</span>{/if}
        {#if !site}<button class="linkish" onclick={onSettings}>Set your location to see distances</button>{/if}
      </div>
    </div>
    <div class="side">
      <div class="tabs" role="tablist">
        <button role="tab" aria-selected={tab === 'now'} onclick={() => goTab('now')}>Overhead now</button>
        <button role="tab" aria-selected={tab === 'messages'} onclick={() => goTab('messages')}>Messages</button>
        <button role="tab" aria-selected={tab === 'earlier'} onclick={() => goTab('earlier')}>Earlier</button>
      </div>
      <div class="scroll" bind:this={list_el}>
        {#if tab === 'now'}
          {#each pickedFirst(rows, pickId) as r (r.a.icao)}
              <button class="row" class:picked={r.a.icao === pickId} class:alarm={isAlarm(r.a)} onclick={() => pickPlane(r.a.icao)}>
                <b>{planeName(r.a)}</b>
                {#if r.km !== null}<span class="r mono">{Math.round(r.km)} km</span>{/if}
                <small class="dim">{[height(r.a), r.a.alt_ft !== null ? trend(r.a) : '', r.a.lat === null ? 'no location yet' : ''].filter(Boolean).join(' · ')}{messages.some((m) => isFrom(m, r.a)) ? ' · has messages' : ''}</small>
              </button>
            {#if r.a.icao === pickId}
              {@const a = r.a}
              <div class="card">

                <p class="dim sub copy">{[a.callsign, a.registration, planeKind(a.category)].filter(Boolean).join(' · ')}</p>
              {#if info?.known}
                <div class="about">
                  {#if info.photo}<img src={info.photo} alt="" />{/if}
                  <div>
                    <b>{[info.maker, info.model].filter(Boolean).join(' ')}</b>
                    {#if info.owner}<span class="dim">{info.owner}</span>{/if}
                  </div>
                </div>
              {/if}
              <div class="links">
                <span class="dim">Look it up:</span>
                {#each lookupLinks(a.icao, a.registration ?? info?.registration) as l (l.label)}
                  <button class="linkish" onclick={() => api.openLink(l.url)}>{l.label}</button>
                {/each}
              </div>
              {#if route}<p class="route">{routeText(route, routeAirports)}</p>{/if}
              {#if isAlarm(a)}<p class="alarm">{a.emergency ?? 'Alert from this plane'}</p>{/if}
              {#if listening}
                <div class="talk">
                  <span class="lbl">Now playing</span>
                  <b>{listening.name}</b>
                  <span class="mono dim small">{listening.channel.mhz.toFixed(3)} MHz</span>
                  <span class="why">You will hear every plane on this channel, not only this one.</span>
                  <button class="btn self" onclick={onStopListening}>Stop listening</button>
                </div>
              {:else if talk}
                <div class="talk">
                  <span class="lbl">Probably talking to</span>
                  <b>{talk.name}</b>
                  <span class="why">{talk.why}. Planes do not say who they talk to, so this is a guess.</span>
                  <button class="btn main self" onclick={() => { frozen = live; onListen(talk!); }}>Listen</button>
                </div>
              {:else}
                <div class="talk">
                  <span class="lbl">Talking to</span>
                  <span class="why">{(a.alt_ft ?? 0) > 18000 ? 'Too high to guess. Planes up here talk to controllers this list does not cover.' : 'No channel known for here.'}</span>
                </div>
              {/if}
              <div class="sec">
                <span class="lbl">Right now</span>
                <dl>
                  {#if a.alt_ft !== null}<dt>Height</dt><dd>{height(a)}, {trend(a)}</dd>{/if}
                  {#if a.speed_kt !== null}<dt>Speed</dt><dd>{Math.round(a.speed_kt * MPH_PER_KT)} mph</dd>{/if}
                  {#if r.km !== null}<dt>From you</dt><dd>{Math.round(r.km)} km</dd>{/if}
                </dl>
              </div>
              <div class="sec">
                <span class="lbl">Messages</span>
                {#each itsMessages as m (m.at_ms + ':' + m.id)}
                  <button class="msg" class:on={pickedMessage === m.id} onclick={() => (pickedMessage = pickedMessage === m.id ? null : m.id)}>
                    <b>{messageTitle(m, knownAirports)}</b>
                    <small class="dim mono">{acarsTime(m.at_ms).slice(0, 5)}{positionsIn(m).length ? ` · ${positionsIn(m).length} on the map` : ''}</small>
                    {#if pickedMessage === m.id}{@render reading(m)}{/if}
                  </button>
                {:else}
                  <span class="why">Nothing heard from this plane yet.</span>
                {/each}
              </div>
              <details class="sec">
                <summary class="lbl">More details</summary>
                <dl>
                  {#each aircraftFacts(a) as f (f.label)}<dt>{f.label}</dt><dd class="copy">{f.value}</dd>{/each}
                </dl>
              </details>
            </div>
            {/if}
          {:else}
              <p class="empty">None heard yet. A line of sight to the sky matters most, so try the antenna by a window.</p>
          {/each}
        {:else if tab === 'messages'}
          <input class="find" bind:value={query} placeholder="Find a flight or a word" aria-label="Find a message" />
          {#each shownMessages as m, i (m.at_ms + ':' + m.id + ':' + m.registration)}
            {@const onMap = planeOf(m)}
            <button class="row" onclick={() => pickRow(m, i)}>
              <b>{messageSender(m)}</b>
              <span class="r mono">{acarsTime(m.at_ms).slice(0, 5)}</span>
              <small class="dim">{messageTitle(m, knownAirports)}{onMap ? ' · on the map' : ''}</small>
              {#if openMessage === i}{@render reading(m)}{/if}
            </button>
          {:else}
            <p class="empty">{query ? 'Nothing matches.' : 'No messages yet. Planes send them every few minutes at busy times, far fewer at night.'}</p>
          {/each}
        {:else}
          <input class="find" bind:value={flightQuery} placeholder="Find a flight" aria-label="Find a flight" />
          {#each shownFlights as d (d.day)}
            <p class="day">{d.day}</p>
            {#each d.flights as f (f.icao + f.first_ms)}
              <button class="row" class:picked={flight?.icao === f.icao && flight.first_ms === f.first_ms} onclick={() => pickFlight(f)}>
                <b>{friendlyCallsign(flightName(f))}</b>
                <span class="r mono">{acarsTime(f.first_ms).slice(0, 5)}</span>
                <small class="dim">{flightLine(f)}</small>
              </button>
            {/each}
          {:else}
            <p class="empty">{flightQuery ? 'Nothing matches.' : 'Nothing yet. Every plane heard stays here for 60 days.'}</p>
          {/each}
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .air {
    position: absolute;
    inset: 0;
    z-index: 20;
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .status {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--muted);
  }
  .ttl {
    font-size: 17px;
    font-weight: 600;
  }
  .grow {
    flex: 1;
  }
  .pill {
    padding: 2px 10px;
    border-radius: 99px;
    font-size: 13px;
    color: var(--green);
    background: color-mix(in srgb, var(--green) 14%, transparent);
  }
  .pill.listen {
    color: var(--yellow);
    background: color-mix(in srgb, var(--yellow) 14%, transparent);
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
  .vol {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .btn {
    padding: 5px 12px;
    border: 1px solid var(--muted);
    border-radius: 6px;
    background: none;
    color: var(--fg);
    font-size: 13px;
    cursor: pointer;
  }
  .btn.main {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--bg-darker);
    font-weight: 600;
  }
  .btn.self {
    justify-self: start;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(280px, 340px);
  }
  @container (max-width: 760px) {
    .body {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
    }
    .side {
      border-left: 0 !important;
      border-top: 1px solid var(--muted);
    }
  }
  .map {
    position: relative;
    min-height: 0;
    min-width: 0;
    background: var(--bg-darker);
  }
  .map.paused :global(svg) {
    opacity: 0.45;
  }
  .about {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .about img {
    width: 96px;
    border-radius: 4px;
  }
  .about div {
    display: flex;
    flex-direction: column;
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
    font-size: 12px;
  }
  .heights i {
    display: inline-block;
    width: 12px;
    height: 3px;
    margin: 0 4px 3px 10px;
    border-radius: 2px;
  }
  .key {
    position: absolute;
    left: 12px;
    bottom: 8px;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .dash {
    display: inline-block;
    width: 18px;
    margin-right: 5px;
    vertical-align: middle;
    border-top: 1.5px dashed var(--yellow);
  }
  .pinkey {
    color: var(--orange);
  }
  .side {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    border-left: 1px solid var(--muted);
  }
  .tabs {
    display: flex;
    gap: 3px;
    padding: 7px;
    border-bottom: 1px solid var(--muted);
  }
  .tabs button {
    padding: 4px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--fg-dim);
    font-size: 13px;
    cursor: pointer;
  }
  .tabs button[aria-selected='true'] {
    background: var(--bg-light);
    color: var(--fg);
    font-weight: 500;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0 10px;
    width: 100%;
    padding: 8px 14px;
    border: 0;
    border-bottom: 1px solid var(--bg-light);
    background: none;
    color: var(--fg);
    font-size: 14px;
    text-align: left;
    cursor: pointer;
  }
  .row:hover,
  .row.picked {
    background: var(--bg-light);
  }
  .row.alarm b {
    color: var(--red);
  }
  .row b {
    font-weight: 500;
  }
  .row small {
    grid-column: 1 / -1;
    font-size: 12.5px;
  }
  .r {
    font-size: 12px;
    color: var(--fg-dim);
    white-space: nowrap;
  }
  .read {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 4px;
    font-size: 12.5px;
  }
  .note {
    font-style: italic;
  }
  .row pre,
  .msg pre {
    grid-column: 1 / -1;
    margin: 4px 0 0;
    font-size: 11.5px;
    color: var(--fg-dim);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .card {
    display: grid;
    gap: 12px;
    padding: 4px 14px 14px;
    background: var(--bg-light);
    border-bottom: 1px solid var(--bg-light);
  }
  .sub {
    margin: 0;
    font-size: 13px;
  }
  .route {
    margin: 0;
    font-weight: 500;
  }
  .alarm {
    margin: 0;
    color: var(--red);
    font-weight: 600;
  }
  .talk {
    display: grid;
    gap: 6px;
    padding: 10px 12px;
    border: 1px dashed var(--yellow);
    border-radius: 8px;
  }
  .lbl {
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--fg-dim);
  }
  .why {
    font-size: 13px;
    color: var(--fg-dim);
  }
  .small {
    font-size: 12px;
  }
  .sec {
    display: grid;
    gap: 5px;
    padding-top: 10px;
    border-top: 1px solid var(--muted);
  }
  summary {
    cursor: pointer;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 14px;
    margin: 0;
    font-size: 14px;
  }
  dt {
    color: var(--fg-dim);
  }
  dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .msg {
    display: grid;
    gap: 2px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: 7px;
    background: var(--bg-light);
    color: var(--fg);
    font-size: 13.5px;
    text-align: left;
    cursor: pointer;
  }
  .msg.on {
    outline: 1.5px solid var(--orange);
  }
  .msg b {
    font-weight: 500;
  }
  .linkish {
    justify-self: start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 13px;
    cursor: pointer;
  }
  .find {
    box-sizing: border-box;
    width: calc(100% - 28px);
    margin: 10px 14px;
    padding: 5px 9px;
    border: 1px solid var(--muted);
    border-radius: 6px;
    background: var(--bg-dark);
    color: var(--fg);
    font-size: 13px;
  }
  .day {
    margin: 8px 14px 2px;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .empty {
    margin: 0;
    padding: 14px;
    font-size: 13.5px;
    color: var(--fg-dim);
  }
  .dim {
    color: var(--fg-dim);
  }
  .mono {
    font-family: var(--mono, ui-monospace, monospace);
    font-variant-numeric: tabular-nums;
  }
</style>
