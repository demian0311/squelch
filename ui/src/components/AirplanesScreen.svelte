<script lang="ts">
  // Everything about aircraft, for someone who has never heard of a band: a map, and each plane with what is known
  // about it from every source. Covers `main`; App owns the radio and tells this screen whether it is listening.
  import AircraftMap from './AircraftMap.svelte';
  import Icon from './Icon.svelte';
  import { untrack } from 'svelte';
  import * as api from '../lib/api';
  import type { Snippet } from 'svelte';
  import type { AcarsMessage, Aircraft, Airport, Flight, PlaneInfo, TrackPoint } from '../lib/api';
  import { acarsTime, filterMessages } from '../lib/acars';
  import { compassLetters, compassWord } from '../lib/radio';
  import { HEIGHT_BANDS, aircraftFacts, fmtClimb, aircraftRows, filterFlights, flightDays, flightLine, flightName, isAlarm, mapFrame, pickedFirst, planeKind, tracksToDraw, trailPiecesKept, type Trails } from '../lib/adsb';
  import { airportLabel, airportsFor, friendlyCallsign, isFrom, lookupLinks, messageReading, messageSender, messageTitle, planeName, positionsIn, routeOf, routeText, talkGuess, type Talk } from '../lib/airplanes';
  import { toKm, type Palette, type Site } from '../lib/terrain';

  interface Props {
    live: Aircraft[];
    /** the radio's waterfall, shown in the map's place while listening */
    waterfall?: Snippet;
    messages: AcarsMessage[];
    trails: Trails;
    site: Site | null;
    listening: Talk | null;
    playing: boolean;
    recording: boolean;
    palette: Palette;
    onListen: (t: Talk) => void;
    onStopListening: () => void;
    onRecord: () => void;
    onSettings: () => void;
  }
  let { live, messages, waterfall, trails, site, listening, playing, recording, palette, onListen, onStopListening, onRecord, onSettings }: Props = $props();

  const FLIGHT_DAYS = 60;
  const FLIGHTS_REFRESH_MS = 60_000;
  const MY_MESSAGES = 8;
  const MPH_PER_KT = 1.15078;

  let tab = $state<'now' | 'messages' | 'earlier'>('now');
  let pickId = $state<string | null>(null);
  let list_el = $state<HTMLElement | undefined>();
  let pickedMessage = $state<number | null>(null);
  /** The open plane's message list and its facts, folded under the card's foot line. */
  let msgsOpen = $state(false);
  let moreOpen = $state(false);
  // while listening the map is what it was when the radio left: nothing here is passed off as live
  let frozen = $state<Aircraft[]>([]);
  // after listening, the planes from before stay until heard again or `CARRY_MS` passes, so the screen is as it was
  const CARRY_MS = 120_000;
  let carried = $state<Aircraft[]>([]);
  let wasListening = false;
  $effect(() => {
    const on = !!listening;
    if (wasListening && !on) {
      carried = untrack(() => frozen);
      setTimeout(() => (carried = []), CARRY_MS);
    }
    wasListening = on;
    if (!on) frozen = live;
  });
  let carriedIds = $derived(new Set(carried.filter((c) => !live.some((a) => a.icao === c.icao)).map((c) => c.icao)));
  // a plane heard again starts with no name until it says it; what it said before fills the gap
  let list = $derived.by(() => {
    if (listening) return frozen;
    const before = new Map(carried.map((c) => [c.icao, c]));
    const named = live.map((a) => {
      const c = before.get(a.icao);
      return c && (!a.callsign || !a.category) ? { ...a, callsign: a.callsign ?? c.callsign, category: a.category ?? c.category, registration: a.registration ?? c.registration } : a;
    });
    return [...named, ...carried.filter((c) => carriedIds.has(c.icao))];
  });
  let rows = $derived(aircraftRows(list, site, true));
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
    msgsOpen = false;
    moreOpen = false;
    flight = null;
    track = [];
    tab = 'now';
  }
  const height = (a: Aircraft) => (a.alt_ft === null ? '' : `${a.alt_ft.toLocaleString('en-US')} ft`);
  const fpm = (v: number) => `${Math.abs(v).toLocaleString('en-US')} ft/min`;
  const msgCount = (a: Aircraft) => messages.filter((m) => isFrom(m, a)).length;

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
  <div class="body">
    <div class="map" class:paused={!!listening}>
      {#if listening && waterfall}
        <div class="wf">{@render waterfall()}</div>
      {/if}
      <div class="mapbody">
      {#if frame}
        <AircraftMap planes={mapPlanes} centre={frame.centre} halfKm={frame.halfKm} here={!!site} tracks={mapTracks} {selected} onpick={pickPlane} onclear={() => { if (pickId) pickPlane(pickId); }} airports={mapAirports} link={mapLink} pins={mapPins} {palette} />
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
        {#if listening}<span>Map paused while you listen</span>{/if}
        {#if !site}<button class="linkish" onclick={onSettings}>Set your location to see distances</button>{/if}
      </div>
      </div>
    </div>
    <div class="side" class:watching={!listening}>
      <div class="tabs" role="tablist">
        <button role="tab" aria-selected={tab === 'now'} onclick={() => goTab('now')} aria-label="Overhead now" title="Overhead now"><Icon name="plane" size={18} /></button>
        <button role="tab" aria-selected={tab === 'messages'} onclick={() => goTab('messages')} aria-label="Messages" title="Messages"><Icon name="msg" size={18} /></button>
        <button role="tab" aria-selected={tab === 'earlier'} onclick={() => goTab('earlier')} aria-label="Earlier" title="Earlier flights"><Icon name="clock" size={18} /></button>
      </div>
      <div class="scroll" bind:this={list_el}>
        {#if tab === 'now'}
          {#each pickedFirst(rows, pickId) as r (r.a.icao)}
              {@const open = r.a.icao === pickId}
              <button class="row" class:picked={r.a.icao === pickId} class:alarm={isAlarm(r.a)} onclick={() => pickPlane(r.a.icao)}>
                <b>{planeName(r.a)}</b>
                {#if r.km !== null}<span class="r mono">{Math.round(r.km)} km</span>{/if}
                <span class="chev"><Icon name={r.a.icao === pickId ? 'chevronDown' : 'chevronRight'} size={14} /></span>
                <!-- closed: height at the left, speed at the right; open: the whole line -->
                <small class="line" class:brief={!open}>
                  {#if r.a.on_ground}<span><Icon name="level" size={13} />on the ground</span>
                  {:else if r.a.alt_ft !== null}
                    <span><Icon name="arrowUp" size={13} />{height(r.a)}</span>
                    {#if open}
                      {#if (r.a.vrate_fpm ?? 0) > 200}<span class="up"><Icon name="climb" size={13} />{fpm(r.a.vrate_fpm!)}</span>
                      {:else if (r.a.vrate_fpm ?? 0) < -200}<span class="down"><Icon name="descend" size={13} />{fpm(r.a.vrate_fpm!)}</span>
                      {:else}<span><Icon name="level" size={13} />level</span>{/if}
                    {/if}
                  {/if}
                  {#if r.a.speed_kt !== null}<span class="speed"><Icon name="gauge" size={13} />{Math.round(r.a.speed_kt * MPH_PER_KT)} mph</span>{/if}
                  {#if open && r.a.heading_deg !== null}<span title={compassWord(r.a.heading_deg)}><i class="arrow" style:transform={`rotate(${Math.round(r.a.heading_deg)}deg)`}><Icon name="arrowUp" size={13} /></i>{compassLetters(r.a.heading_deg)}</span>{/if}
                  {#if open && msgCount(r.a)}<span><Icon name="msg" size={13} />{msgCount(r.a)}</span>{/if}
                  {#if carriedIds.has(r.a.icao)}<span class="note">from before you listened</span>{:else if r.a.lat === null}<span class="note">no location yet</span>{/if}
                </small>
              </button>
            {#if r.a.icao === pickId}
              {@const a = r.a}
              <div class="card">
              <div class="about">
                {#if info?.photo}<img src={info.photo} alt="" />{/if}
                <div class="what">
                  {#if info?.known}<b>{[info.maker, info.model].filter(Boolean).join(' ')}</b>{/if}
                  <ul class="bits dim copy">
                    {#each [info?.owner, a.callsign, a.registration, planeKind(a.category)].filter((x) => x && x !== planeName(a)) as bit (bit)}<li>{bit}</li>{/each}
                  </ul>
                </div>
                <div class="links">
                  {#each lookupLinks(a.icao, a.registration ?? info?.registration) as l (l.label)}
                    <button class="linkish" onclick={() => api.openLink(l.url)} title="Opens in the browser">{l.label}<Icon name="external" size={12} /></button>
                  {/each}
                </div>
              </div>
              {#if route}<p class="route">{routeText(route, routeAirports)}</p>{/if}
              {#if isAlarm(a)}<p class="alarm">{a.emergency ?? 'Alert from this plane'}</p>{/if}
              <!-- the foot line: the channel (a guess, as loud as a link), the messages, and the rest folded under "More" -->
              <div class="foot">
                {#if listening || talk}
                  {@const t = (listening ?? talk)!}
                  <span class="ch">
                    <button class="play" class:on={!!listening && playing} onclick={() => { if (listening) onStopListening(); else { frozen = live; onListen(t); } }} aria-label={listening ? 'Stop listening' : 'Listen'} title={listening ? 'Stop listening: the map comes back. Every plane on this channel, not only this one.' : `Listen. A guess: ${t.why.toLowerCase()}; planes do not say who they talk to.`}>
                      <Icon name={listening && playing ? 'pause' : 'play'} size={12} />
                    </button>
                    {t.name} <span class="mono dim">{t.channel.mhz.toFixed(3)}</span>
                  </span>
                  {#if listening}
                    <button class="rec" class:on={recording} onclick={onRecord} aria-label={recording ? 'Stop recording' : 'Record'} title={recording ? 'Stop recording' : 'Record what you hear to a file'}>
                      <Icon name="record" size={14} />
                    </button>
                  {/if}
                {:else}
                  <span class="dim" title={(a.alt_ft ?? 0) > 18000 ? 'Planes this high talk to controllers this list does not cover' : 'No channel known for here'}><Icon name="tower" size={13} />{(a.alt_ft ?? 0) > 18000 ? 'too high for a channel' : 'no channel known'}</span>
                {/if}
                {#if itsMessages.length}
                  <button class="fold" class:on={msgsOpen} onclick={() => (msgsOpen = !msgsOpen)}><Icon name="msg" size={13} />{itsMessages.length} {itsMessages.length === 1 ? 'message' : 'messages'}</button>
                {:else}
                  <span class="dim"><Icon name="msg" size={13} />no messages yet</span>
                {/if}
                <button class="fold more" class:on={moreOpen} onclick={() => (moreOpen = !moreOpen)}>More<Icon name={moreOpen ? 'chevronDown' : 'chevronRight'} size={13} /></button>
              </div>
              {#if msgsOpen && itsMessages.length}
              <div class="sec">
                {#each itsMessages as m (m.at_ms + ':' + m.id)}
                  <button class="msg" class:on={pickedMessage === m.id} onclick={() => (pickedMessage = pickedMessage === m.id ? null : m.id)}>
                    <b>{messageTitle(m, knownAirports)}</b>
                    <small class="dim mono">{acarsTime(m.at_ms).slice(0, 5)}{positionsIn(m).length ? ` · ${positionsIn(m).length} on the map` : ''}</small>
                    {#if pickedMessage === m.id}{@render reading(m)}{/if}
                  </button>
                {/each}
              </div>
              {/if}
              {#if moreOpen}
              <div class="sec">
                <dl>
                  {#each aircraftFacts(a) as f (f.label)}<dt>{f.label}</dt><dd class="copy">{f.value}</dd>{/each}
                </dl>
              </div>
              {/if}
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
      {#if !listening && waterfall}
        <div class="sidewf">{@render waterfall()}</div>
      {/if}
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
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    background: var(--bg-darker);
  }
  .mapbody {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .map.paused :global(svg) {
    opacity: 0.45;
  }
  /* the waterfall takes the top of the map's column; the paused map keeps the rest */
  .wf {
    position: relative;
    flex: 0 0 42%;
    min-height: 160px;
    border-bottom: 1px solid var(--muted);
  }
  /* the waterfall fills its box; left to size itself, the canvas would set its own height */
  .wf > :global(*) {
    position: absolute;
    inset: 0;
  }
  .play,
  .rec {
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: transparent;
    cursor: pointer;
  }
  .play {
    width: 40px;
    height: 40px;
    border: 1px solid var(--accent);
    color: var(--accent);
  }
  .play.on {
    background: var(--accent);
    color: var(--bg-darker);
  }
  .rec {
    width: 32px;
    height: 32px;
    border: 1px solid var(--muted);
    color: var(--red);
  }
  .rec.on {
    border-color: var(--red);
    background: var(--red);
    color: var(--bg-darker);
  }
  .play :global(svg),
  .rec :global(svg) {
    fill: currentColor;
  }
  .about {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 10px;
    align-items: center;
  }
  .about img {
    width: 96px;
    border-radius: 4px;
  }
  .about .what {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .bits {
    margin: 2px 0 0;
    padding-left: 16px;
    font-size: 12px;
    line-height: 1.5;
  }
  .links {
    display: grid;
    gap: 5px;
    justify-items: end;
    font-size: 12px;
  }
  .links .linkish {
    display: inline-flex;
    align-items: center;
    gap: 3px;
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
    display: grid;
    place-items: center;
    padding: 5px 12px;
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
  /* while watching, the radio's waterfall takes what the list leaves and never shrinks past about two rows */
  .watching .scroll {
    flex: 0 1 auto;
  }
  .sidewf {
    position: relative;
    flex: 1 1 0;
    min-height: 118px;
    border-top: 1px solid var(--muted);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
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
  .row .chev {
    display: grid;
    align-self: center;
    color: var(--fg-dim);
  }
  .row small {
    grid-column: 1 / -1;
    font-size: 12.5px;
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    gap: 3px 13px;
    align-items: center;
    margin-top: 3px;
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    color: var(--fg-dim);
  }
  .line span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .line .up {
    color: var(--green);
  }
  .line .down {
    color: var(--orange);
  }
  .line .arrow {
    display: inline-grid;
  }
  .line .note {
    font-family: inherit;
    font-style: italic;
  }
  .line.brief .speed {
    order: 1;
    margin-left: auto;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    align-items: center;
    font-size: 12.5px;
  }
  .foot > * {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .foot .ch .play {
    width: 22px;
    height: 22px;
    border: 1.5px solid var(--accent);
    color: var(--accent);
  }
  .foot .ch .play.on {
    background: var(--accent);
    color: var(--bg);
  }
  .foot .rec {
    width: 22px;
    height: 22px;
    border: 1.5px solid var(--red);
    color: var(--red);
  }
  .foot .rec.on {
    background: var(--red);
    color: var(--bg);
  }
  .fold {
    padding: 0;
    border: 0;
    background: none;
    color: var(--fg-dim);
    font: inherit;
    cursor: pointer;
  }
  .fold.on,
  .fold:hover {
    color: var(--fg);
  }
  .fold.more {
    margin-left: auto;
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
    grid-template-columns: minmax(0, 1fr); /* auto would size to the about row's max-content and run off the panel */
    gap: 12px;
    padding: 4px 14px 14px;
    background: var(--bg-light);
    border-bottom: 1px solid var(--bg-light);
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
  .sec {
    display: grid;
    gap: 5px;
    padding-top: 10px;
    border-top: 1px solid var(--muted);
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
