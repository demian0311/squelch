<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import * as api from './lib/api';
  import type { Band, BandPlan, Mode, Setup, Status, Theme } from './lib/api';
  import { allBookmarks, bandAt, fmtHz, fmtMHz, label, modeAt, Persistence, placeCards, snapTune, sondeDrift, sondeName, sondeSummary, withSondes, type Saved, type Signal } from './lib/radio';
  import { apply as applyTheme, resolve, waterfallStops } from './lib/theme';
  import BandRail from './components/BandRail.svelte';
  import Waterfall from './components/Waterfall.svelte';
  import Icon from './components/Icon.svelte';

  const BANDWIDTH: Record<Mode, number> = { wfm: 200_000, nfm: 12_500, am: 8_000, usb: 2_800, lsb: 2_800, none: 10_000 };
  const CARD_W = 176;
  const LANE_H = 92;
  const SPEC_H = 130;
  const MIN_SPAN = 25_000;
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
  let volume = $state(0.7);
  let gain = $state<string>('auto');
  let railOpen = $state(true);
  let dockOpen = $state(readDock());
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
  function readDock(): boolean {
    try {
      return localStorage.getItem('dock') !== 'closed';
    } catch {
      return true;
    }
  }
  function toggleDock() {
    dockOpen = !dockOpen;
    try {
      localStorage.setItem('dock', dockOpen ? 'open' : 'closed');
    } catch {
      // remembering the choice is a nicety
    }
  }
  let full = $state(false);
  let boxW = $state(1000);
  let waterfall: Waterfall;
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

  function pickBand(b: Band) {
    bandId = b.id;
    const s = Math.min(maxSpan, (b.high_mhz - b.low_mhz) * 1e6);
    const start = b.start_mhz * 1e6;
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
      (status?.signals ?? []).map((d) => label(plan, saved, d)),
      status?.sondes ?? [],
    ).map((s) => ({ ...s, steady: persist.steady(s, Date.now(), UPDATES_PER_WINDOW) })),
  );
  let steadyCount = $derived(signals.filter((s) => s.steady && s.hz >= vs && s.hz <= ve).length);
  let listening = $derived(signals.find((s) => Math.abs(s.hz - tuned) <= Math.max(s.widthHz / 2, 3_000)));
  let listeningMark = $derived(saved.find((s) => Math.abs(s.mhz * 1e6 - tuned) < 1_500));

  let cards = $derived.by(() => {
    const inView = signals.filter((s) => s.hz >= vs && s.hz <= ve && (showSteady || !s.steady));
    const minSep = ((CARD_W + 8) / boxW) * span;
    return placeCards(inView, minSep, (s) => s === listening).map((p) => {
      const x = ((p.item.hz - vs) / span) * boxW;
      return {
        ...p,
        x,
        left: Math.min(Math.max(x - CARD_W / 2, 6), boxW - CARD_W - 6),
        meter: Math.min(100, Math.round((p.item.snrDb / 40) * 100)),
        selected: p.item === listening,
      };
    });
  });

  let dots = $derived.by(() => {
    const hidden = cards.flatMap((c) => c.hidden.map((h) => ({ x: ((h.hz - vs) / span) * boxW, cls: 'hidden', title: `${fmtMHz(h.hz)} ${h.title} — behind a stronger signal; zoom in` })));
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
    if (next !== undefined) tuneTo(snapTune(next, status?.signals ?? [], saved, span / boxW, mode));
  }

  // ── weather balloons ────────────────────────────

  let decoding = $derived(band?.decoder === 'radiosonde');
  let sondes = $derived(status?.sondes ?? []);
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

  let nowTitle = $derived(listeningMark?.name ?? listening?.title ?? (band ? band.name : 'Tuned'));
  let muted = $derived(playing && status?.squelch_open === false);
  let nowSub = $derived(
    muted ? 'Muted — signal is below the squelch level' :
    listening ? listening.sub : listeningMark ? 'Quiet right now' : 'Between signals — press → for the next one',
  );
  let meter = $derived(playing ? Math.max(0, Math.min(100, ((status?.channel_snr_db ?? 0) / 40) * 100)) : 0);

  // ── theme ───────────────────────────────────────────

  let stops = $derived(waterfallStops(theme));
  let wfColors = $derived({
    bg: theme.darker_background,
    grid: theme.lighter_background,
    line: theme.foreground,
    fill: theme.accent + '2e',
    accent: theme.accent,
    tick: theme.light_foreground,
  });

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

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
    const keyed = plan.groups.flatMap((g) => plan.bands.filter((b) => b.group === g.id)).slice(0, 9);
    if (/^[1-9]$/.test(e.key) && keyed[+e.key - 1]) pickBand(keyed[+e.key - 1]);
    else if (e.key === 'ArrowRight') step(1);
    else if (e.key === 'ArrowLeft') step(-1);
    else if (e.key === '[') setView(vs - span / 4, ve - span / 4);
    else if (e.key === ']') setView(vs + span / 4, ve + span / 4);
    else if (e.key === '-') zoom(1.5);
    else if (e.key === '=' || e.key === '+') zoom(1 / 1.5);
    else if (e.key === 'f') toggleFull();
    else if (e.key === ' ') togglePlay();
    else if (e.key === 'c') toggleDock();
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
      await api.start(
        (row) => waterfall?.push(row),
        (s) => {
          persist.update(s.signals.map((d) => d.hz), Date.now());
          status = s;
        },
      );
      const fm = plan.bands.find((x) => x.id === 'fm') ?? plan.bands[0];
      if (fm) pickBand(fm);
      unsubs.push(api.onTheme(setTheme));
      unsubs.push(
        api.onBands((nb) => {
          plan = nb.plan;
          bandsPath = nb.path;
          bandsError = nb.error;
        }),
      );
    })();
    return () => unsubs.forEach((u) => u.then((f) => f()));
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
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <header>
    <div class="freq">
      <span class="digits mono">{fmtHz(tuned)}</span>
      <span class="unit mono">MHz</span>
    </div>
    <div class="now">
      <span class="title">{nowTitle}</span>
      <span class="sub">{nowSub}</span>
    </div>
    <div class="meter">
      <span>Signal</span>
      <div class="track">
        <div class="fill" class:muted style:width="{meter}%"></div>
        {#if squelch > 0}<span class="sq-mark" style:left="{Math.min(100, (squelch / 40) * 100)}%" title="Squelch level"></span>{/if}
      </div>
    </div>
  </header>

  <div class="body">
    {#if !full}
      <BandRail {plan} current={bandId} open={railOpen} onPick={pickBand} onToggle={() => (railOpen = !railOpen)} />
    {/if}

    <main>
      <div class="toolbar">
        <h1>{band?.name ?? 'Receiver'}</h1>
        <span class="range mono">{fmtMHz(vs)} – {fmtMHz(ve)} MHz</span>
        {#if decoding}
          <span class="hint">Decoding weather balloons (DFM and RS41) on {status?.decoding_hz.length ?? 0} signal{status?.decoding_hz.length === 1 ? '' : 's'}</span>
        {:else if band?.decoder}
          <span class="note">The {band.decoder} decoder is not built yet — this shows the raw signal.</span>
        {:else}
          <span class="hint">Drag to move · scroll to zoom · click to tune</span>
        {/if}
        {#if steadyCount || showSteady}
          <button class="steadybtn" onclick={() => (showSteady = !showSteady)} title="Narrow carriers that never switch off are usually interference from nearby electronics">
            {showSteady ? 'Hide steady carriers' : `${steadyCount} steady carrier${steadyCount === 1 ? '' : 's'} hidden · show`}
          </button>
        {/if}
      </div>

      <div class="stage">
      <div class="box" bind:clientWidth={boxW}>
        <div class="lane" style:height="{LANE_H}px">
          {#each dots as d}
            <span class="dot {d.cls}" style:left="{d.x}px" title={d.title}></span>
          {/each}
          {#each cards as c (c.item.hz)}
            <span class="stem" class:sel={c.selected} style:left="{c.x}px"></span>
            <button class="card" class:sel={c.selected} style:left="{c.left}px" style:width="{CARD_W}px" onclick={() => tuneTo(snapTune(c.item.hz, status?.signals ?? [], saved, span / boxW, mode))}>
              <span class="row1">
                <span class="ttl">{c.item.title}</span>
                {#if c.hidden.length}
                  <span class="more mono" title={c.hidden.map((h) => `${fmtMHz(h.hz)} ${h.title}`).join(', ') + ' — zoom in to separate'}>+{c.hidden.length}</span>
                {/if}
              </span>
              <span class="row2">{c.item.sub}</span>
              <span class="row3 mono">
                <span>{fmtMHz(c.item.hz)}</span>
                <span class="tag" class:saved={c.item.saved}>{c.item.tag}</span>
                <span class="bar"><span style:width="{c.meter}%"></span></span>
              </span>
            </button>
          {/each}
        </div>
        <div class="wf">
          <Waterfall
            bind:this={waterfall}
            {vs}
            {ve}
            {rate}
            {stops}
            {tuned}
            bandwidth={BANDWIDTH[mode]}
            specH={SPEC_H}
            colors={wfColors}
            onTune={(hz, pxHz) => tuneTo(snapTune(hz, status?.signals ?? [], saved, pxHz, mode))}
            onView={setView}
          />
        </div>
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
      </div>
      {#if decoding}
        <aside class="sondes">
          <h2>Weather balloons</h2>
          {#each sondes as o (o.hz)}
            {@const drift = sondeDrift(o)}
            <button class="sonde" class:sel={Math.abs(o.hz - tuned) < 10_000} onclick={() => tuneTo(o.hz)}>
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
        </aside>
      {/if}
      </div>
    </main>
  </div>

  <footer class:open={dockOpen}>
    <div class="dockbar">
      <button class="fold" onclick={toggleDock} aria-expanded={dockOpen} title={dockOpen ? 'Hide controls (C)' : 'Show controls (C)'}>
        {dockOpen ? '▾' : '▴'}
      </button>
      {#if dockOpen}
        <label class="ctl">
          Volume
          <input type="range" min="0" max="1.5" step="0.05" bind:value={volume} />
        </label>
        <label class="ctl" title="Mutes the audio while the signal is weaker than this. Far left turns it off.">
          Squelch
          <input type="range" min="0" max="40" step="1" bind:value={squelch} />
          <span class="val mono">{squelch > 0 ? `${squelch} dB` : 'Off'}</span>
        </label>
        <label class="ctl">
          Gain
          <select bind:value={gain}>
            <option value="auto">Auto</option>
            {#each status?.gains ?? [] as g}
              <option value={String(g)}>{(g / 10).toFixed(1)} dB</option>
            {/each}
          </select>
        </label>
        <div class="transport">
          <button class="sq" onclick={() => step(-1)} aria-label="Previous signal" title="Previous signal (←)">←</button>
          <button class="play" class:on={playing} onclick={togglePlay}>{playing ? 'Stop' : 'Listen'}</button>
          <button class="sq" onclick={() => step(1)} aria-label="Next signal" title="Next signal (→)">→</button>
        </div>
        <span class="spacer"></span>
        <div class="tools">
          <button onclick={() => zoom(1.5)} aria-label="Zoom out" title="Zoom out (-)">−</button>
          <button onclick={() => zoom(1 / 1.5)} aria-label="Zoom in" title="Zoom in (+)">+</button>
          <button class="wide" onclick={() => setView((vs + ve) / 2 - maxSpan / 2, (vs + ve) / 2 + maxSpan / 2)}>Widest</button>
          <button class="wide accent" onclick={toggleFull} title="Full screen (F)">
            <Icon name={full ? 'shrink' : 'expand'} size={16} />{full ? 'Exit full screen' : 'Full screen'}
          </button>
        </div>
      {:else}
        <button class="play small" class:on={playing} onclick={togglePlay}>{playing ? 'Stop' : 'Listen'}</button>
        <span class="spacer"></span>
        <span class="info mono" class:warn={status?.simulated}>{status?.device ?? 'Starting…'}</span>
      {/if}
    </div>
    {#if dockOpen}
      <div class="info mono">
        <span>1–9 band</span>
        <span>← → next signal</span>
        <span>[ ] move · - + zoom</span>
        <span>f full screen</span>
        <span>space listen / stop</span>
        <span>c controls</span>
        <span class="spacer"></span>
        {#if status?.error}<span class="err">{status.error}</span>{/if}
        {#if bandsError}<span class="err">Bands file: {bandsError}</span>{/if}
        <span class:warn={status?.simulated}>{status?.device ?? 'Starting…'}</span>
        <span title={bandsPath ?? ''}>Bands from {bandsPath?.replace(/^\/home\/[^/]+/, '~') ?? 'built-in list'}</span>
        <span>Colours: {theme.name ?? 'Omarchy theme'}</span>
      </div>
    {:else if status?.error || bandsError}
      <div class="info mono">
        {#if status?.error}<span class="err">{status.error}</span>{/if}
        {#if bandsError}<span class="err">Bands file: {bandsError}</span>{/if}
      </div>
    {/if}
  </footer>
</div>

<style>
  .app {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px 22px;
    padding: 10px 18px;
    background: var(--bg-dark);
    border-bottom: 1px solid var(--muted);
  }
  .freq {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .digits {
    font-size: 38px;
    font-weight: 600;
    letter-spacing: -1px;
    color: var(--fg-bright);
  }
  .unit {
    color: var(--fg-dim);
  }
  .now {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1 1 240px;
    min-width: 0;
  }
  .now .title {
    font-weight: 600;
    font-size: 16px;
    color: var(--fg-bright);
  }
  .now .sub {
    color: var(--fg-dim);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meter {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 150px;
    color: var(--fg-dim);
    font-size: 12px;
  }
  .track {
    position: relative;
    height: 8px;
    border-radius: 4px;
    background: var(--muted);
  }
  .fill.muted {
    background: var(--fg-dim);
  }
  .sq-mark {
    position: absolute;
    top: -3px;
    width: 2px;
    height: 14px;
    margin-left: -1px;
    background: var(--yellow);
  }
  .val {
    min-width: 44px;
    font-size: 12px;
  }
  .fill {
    height: 8px;
    background: var(--green);
    transition: width 200ms;
  }
  .ctl {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--fg-dim);
    font-size: 12px;
  }
  .ctl input {
    width: 110px;
    accent-color: var(--accent);
  }
  .ctl select {
    background: var(--bg-light);
    border: 1px solid var(--muted);
    border-radius: 6px;
    padding: 2px 6px;
  }
  .transport {
    display: flex;
    gap: 8px;
  }
  .sq {
    min-height: 44px;
    min-width: 44px;
    border-radius: 8px;
    border: 1px solid var(--muted);
    background: transparent;
    font-size: 16px;
  }
  .play {
    min-height: 44px;
    min-width: 96px;
    border-radius: 8px;
    border: 1px solid var(--accent);
    background: transparent;
    color: var(--accent);
    font-weight: 600;
  }
  .play.on {
    background: var(--accent);
    color: var(--bg-darker);
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 16px;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
  }
  h1 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }
  .range,
  .hint {
    color: var(--fg-dim);
    font-size: 13px;
  }
  .note {
    color: var(--yellow);
    font-size: 13px;
  }
  .tools {
    display: flex;
    gap: 6px;
  }
  .tools button {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 34px;
    min-width: 38px;
    justify-content: center;
    border-radius: 8px;
    border: 1px solid var(--muted);
    background: transparent;
    font-size: 15px;
  }
  .tools .wide {
    padding: 0 12px;
    font-size: 13px;
  }
  .tools .accent {
    border-color: var(--accent);
    color: var(--accent);
  }
  .stage {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 12px;
  }
  .sondes {
    flex: 0 0 260px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
  }
  .sondes h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--fg-dim);
  }
  .sonde {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
    text-align: left;
  }
  .sonde.sel {
    border-color: var(--accent);
    background: var(--sel);
  }
  .sonde .name {
    font-weight: 600;
    color: var(--fg-bright);
  }
  .sonde .big {
    font-size: 16px;
    color: var(--accent);
  }
  .sonde .dim,
  .empty {
    font-size: 12px;
    color: var(--fg-dim);
  }
  .empty {
    margin: 0;
    line-height: 1.45;
  }
  .box {
    min-width: 0;
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-radius: 10px;
    overflow: hidden;
    border: 1px solid var(--muted);
    background: var(--bg-darker);
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
  .steadybtn {
    border: 0;
    background: transparent;
    color: var(--fg-dim);
    font-size: 12px;
    text-decoration: underline dotted;
  }
  .stem {
    position: absolute;
    top: 74px;
    bottom: 0;
    width: 1px;
    background: var(--muted);
  }
  .stem.sel {
    background: var(--accent);
  }
  .card {
    position: absolute;
    top: 8px;
    height: 66px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 7px 10px;
    border-radius: 8px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
    text-align: left;
    overflow: hidden;
    transition: left 150ms;
  }
  .card.sel {
    border-color: var(--accent);
    background: var(--sel);
  }
  .row1 {
    display: flex;
    align-items: baseline;
    gap: 6px;
    width: 100%;
  }
  .ttl {
    flex: 1;
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
    font-size: 11px;
    color: var(--fg-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row3 {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    font-size: 10px;
    color: var(--fg-dim);
  }
  .tag.saved {
    color: var(--cyan);
  }
  .bar {
    margin-left: auto;
    width: 36px;
    height: 3px;
    border-radius: 2px;
    background: var(--muted);
  }
  .bar span {
    display: block;
    height: 3px;
    border-radius: 2px;
    background: var(--green);
  }
  .card.sel .bar span {
    background: var(--accent);
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
  footer {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 6px 18px 8px;
    background: var(--bg-dark);
    border-top: 1px solid var(--muted);
  }
  .dockbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 22px;
  }
  .dockbar .ctl {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }
  .fold {
    min-width: 34px;
    min-height: 34px;
    border-radius: 8px;
    border: 1px solid var(--muted);
    background: transparent;
    color: var(--fg-dim);
  }
  .play.small {
    min-height: 34px;
    min-width: 80px;
  }
  .info {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 22px;
    color: var(--fg-dim);
    font-size: 12px;
  }
  .spacer {
    flex: 1;
  }
  .err {
    color: var(--red);
  }
  .warn {
    color: var(--yellow);
  }
</style>
