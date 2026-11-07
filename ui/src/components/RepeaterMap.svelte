<script lang="ts">
  // Repeaters around the listener. Colour answers one question, can it be heard here (green yes, blue maybe
  // digital only, red probably not); the tuned one is twice the size. Scroll zooms around the pointer, drag pans,
  // never wider than there is anything to hear; marks too close to tell apart fold into one badge that opens on a click.
  import { untrack } from 'svelte';
  import { placeLabels } from '../lib/airplanes';
  import type { Mark, MapView, RepeaterPoint } from '../lib/repeaters';
  import { APART_PX, NEAREST_KM, REP_LIMITS, WIDEST_KM, badgeRadius, clampView, hueOf, mapMarks, openHalfKm, ringReach, townLabels, zoomView } from '../lib/repeaters';
  import { mapGestures } from '../lib/mapview';
  import MapZoom from './MapZoom.svelte';
  import { groundKey, linePaths, type Frame, type Lines, type Palette, type Site } from '../lib/terrain';

  interface Props {
    points: RepeaterPoint[];
    reachKm: number;
    tunedId: string | null;
    /** output frequencies with a signal on them now, and how strong */
    live: Map<number, number>;
    onTune: (hz: number) => void;
    view: MapView;
    onView: (v: MapView) => void;
    site: Site;
    palette: Palette;
  }
  let { points, reachKm, tunedId, live, onTune, view, onView, site, palette }: Props = $props();

  // the ground is drawn once for the widest view, wider than tall so a wide panel has ground to its edges
  const WIDTH_RATIO = 8 / 3;
  const DISTANCES_KM = [2, 5, 10, 25, 50];
  // dots and words keep their size on screen whatever the panel's
  const DOT_PX = 5;
  const HIT_PX = 13;
  const LABEL_PX = 11;
  const TOWN_PX = 12;
  const YOU_PX = 13;
  const DIST_PX = 10;
  let box = $state<DOMRectReadOnly | undefined>();
  let halfKm = $derived(view.halfKm);
  let halfW = $derived(halfKm * (box?.width && box.height ? box.width / box.height : WIDTH_RATIO));
  let kmPerPx = $derived(box?.height ? (2 * halfKm) / box.height : 1);
  let distanceKm = $derived(DISTANCES_KM.filter((d) => d <= halfKm * 0.9).at(-1) ?? DISTANCES_KM[0]);
  let nearest = $derived(halfKm <= NEAREST_KM * 1.01);
  const level = GROUNDS[0];
  let frame = $derived<Frame>({ site: { lat: site.lat, lon: site.lon }, halfWKm: WIDEST_KM * WIDTH_RATIO, halfHKm: WIDEST_KM });

  // state lines and highways ship with the app; loaded on first use, then kept
  let lines = $state.raw<Lines | null>(cachedLines);
  let paths = $derived(lines ? linePaths(lines, frame) : null);
  $effect(() => {
    if (!lines) loadLines().then((l) => (lines = l));
  });

  // shaded terrain: best effort. Memory, then the disk cache, then drawn from tiles and saved; without tiles the lines draw alone
  let ground = $state<{ url: string; frame: Frame } | null>(null);
  $effect(() => {
    const f = frame;
    const p = { ...palette };
    const key = groundKey(f, p);
    let stale = false;
    const cached = groundCache.get(key);
    const old = untrack(() => ground);
    if (cached) ground = { url: cached, frame: f };
    else if (old && (old.frame.site.lat !== f.site.lat || old.frame.site.lon !== f.site.lon)) ground = null;
    if (!cached)
      (async () => {
        let url = await api.ground(key).catch(() => null);
        if (!url) {
          url = await drawGround(f, p, level);
          if (url) api.saveGround(key, url).catch(() => {});
        }
        if (url) groundCache.set(key, url);
        if (!stale && url) ground = { url, frame: f };
      })();
    return () => (stale = true);
  });

  // what the view leaves on screen, with a margin so a badge near the edge still forms
  let inView = $derived.by(() => {
    const m = HIT_PX * 2 * kmPerPx;
    return points.filter((p) => Math.abs(p.x - view.x) <= halfW + m && Math.abs(p.y - view.y) <= halfKm + m);
  });
  let towns = $derived(townLabels(inView, 8));
  let tuned = $derived(points.find((p) => p.id === tunedId));
  // a shared frequency: what is on the air is taken to be the nearest one, and `points` is nearest first
  let onAir = $derived.by(() => {
    const seen = new Set<number>();
    return points.filter((p) => live.has(p.hz) && !seen.has(p.hz) && seen.add(p.hz));
  });
  let marks = $derived(mapMarks(inView, kmPerPx, nearest, new Set([tunedId, ...onAir.map((p) => p.id)])));
  // the tuned repeater draws last, on top
  let ordered = $derived([...marks].sort((a, b) => Number(a.kind === 'one' && a.p.id === tunedId) - Number(b.kind === 'one' && b.p.id === tunedId)));
  const radius = (m: Mark) => (m.kind === 'many' ? badgeRadius(m.points.length) : m.p.id === tunedId ? APART_PX : DOT_PX);
  // rings run only on what can be seen
  let ringing = $derived(onAir.filter((p) => inView.includes(p)));
  const place = (p: RepeaterPoint) => `${p.call} · ${p.city} · ${(p.hz / 1e6).toFixed(4)} MHz · ${Math.round(p.km)} km`;
  const onAirNote = (p: RepeaterPoint) => (onAir.includes(p) ? ` · on air, ${live.get(p.hz)!.toFixed(0)} dB above the noise` : '');
  const hearing = (p: RepeaterPoint) => (p.digital === 'DMR' ? `DMR · plays here${p.reach ? '' : ', though probably too far'}` : p.reach ? 'FM · you should hear it' : 'FM · probably too far');
  const title = (p: RepeaterPoint) => `${place(p)}${onAirNote(p)} · ${hearing(p)}`;
  const badgeTitle = (ps: RepeaterPoint[]) => `${ps.length} repeaters near ${ps[0].city} · click to open\n${ps.slice(0, 8).map((p) => `${p.call} ${(p.hz / 1e6).toFixed(3)}`).join('\n')}${ps.length > 8 ? `\n… ${ps.length - 8} more` : ''}`;

  // Words go wherever they fit without covering a mark or each other, what matters most placed first: the tuned and
  // on-air calls, then the towns, then every other mark nearest first. A badge's word is its nearest call and how many more.
  const markId = (m: Mark) => (m.kind === 'one' ? m.p.id : m.points[0].id);
  const markWord = (m: Mark) => (m.kind === 'one' ? m.p.call : `${m.points[0].call} +${m.points.length - 1}`);
  let labels = $derived.by(() => {
    const u = kmPerPx;
    const items: Parameters<typeof placeLabels>[0] = [];
    items.push({ id: 'you', x: (DOT_PX + 4) * u, y: (4 - YOU_PX * 0.78) * u, text: 'You', fixed: true });
    for (const m of ordered) items.push({ id: `mark:${markId(m)}`, x: m.x, y: m.y, text: '', r: radius(m) });
    const rank = (m: Mark) => (m.kind === 'many' ? 2 : m.p.id === tunedId ? 0 : onAir.includes(m.p) ? 1 : 2);
    const km = (m: Mark) => (m.kind === 'one' ? m.p.km : m.points[0].km);
    const byRank = [...ordered].sort((a, b) => rank(a) - rank(b) || km(a) - km(b));
    const word = (m: Mark) => ({ id: markId(m), x: m.x, y: m.y, text: markWord(m), r: radius(m) });
    for (const m of byRank) if (rank(m) <= 1) items.push(word(m));
    // a town's name sits at the middle of its repeaters, which is usually a badge: it keeps clear of whatever is there
    for (const t of towns) items.push({ id: `town:${t.name}`, x: t.x, y: t.y, text: t.name, r: Math.max(DOT_PX, ...ordered.filter((m) => Math.hypot(m.x - t.x, m.y - t.y) < 20 * u).map(radius)) });
    for (const m of byRank) if (rank(m) > 1) items.push(word(m));
    return placeLabels(items, u, LABEL_PX * 0.62, LABEL_PX + 2);
  });

  function onKey(e: KeyboardEvent, go: () => void) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      go();
    }
  }

  // ---- zoom and pan, like a street map ----
  let svg = $state<SVGSVGElement>();
  let dragging = $state(false);
  const g = mapGestures({ view: () => view, kmPerPx: () => kmPerPx, svg: () => svg, limits: () => REP_LIMITS, onView: (v) => onView(v), onDragging: (d) => (dragging = d) });
  function pick(go: () => void) {
    if (g.dragged()) return;
    go();
  }
  // one click takes a badge apart: the view closes on it just enough, and its tower comes to the centre
  const open = (m: Extract<Mark, { kind: 'many' }>) => onView(clampView({ x: m.from?.x ?? m.x, y: m.from?.y ?? m.y, halfKm: openHalfKm(m.points, kmPerPx, halfKm) }));
</script>

<script lang="ts" module>
  import * as api from '../lib/api';
  import type { Lines as L } from '../lib/terrain';
  import { GROUNDS, drawGround, groundCache } from '../lib/ground';

  let cachedLines: L | null = null;
  async function loadLines(): Promise<L> {
    cachedLines ??= (await import('../lib/maplines.json')).default as unknown as L;
    return cachedLines;
  }
</script>

<svg
  bind:this={svg}
  bind:contentRect={box}
  viewBox="{view.x - halfW} {view.y - halfKm} {2 * halfW} {2 * halfKm}"
  preserveAspectRatio="none"
  role="group"
  aria-label="Repeaters around you. Scroll to zoom, drag to move, press one to tune it."
  class:dragging={dragging}
  onwheel={g.onwheel}
  onpointerdown={g.onpointerdown}
  onpointermove={g.onpointermove}
  onpointerup={g.onpointerup}
  onpointercancel={g.onpointerup}
  ondblclick={g.ondblclick}
>
  <rect x={-frame.halfWKm} y={-frame.halfHKm} width={2 * frame.halfWKm} height={2 * frame.halfHKm} class="ground" />
  {#if ground}
    <image href={ground.url} x={-ground.frame.halfWKm} y={-ground.frame.halfHKm} width={2 * ground.frame.halfWKm} height={2 * ground.frame.halfHKm} preserveAspectRatio="none" />
  {/if}
  {#if paths}
    <path d={paths.minor} class="road minor" />
    <path d={paths.major} class="road major" />
    <path d={paths.states} class="state" />
  {/if}
  <circle r={distanceKm} class="distance" />
  <text x={distanceKm * 0.71 + 2 * kmPerPx} y={-distanceKm * 0.71 - 2 * kmPerPx} class="distlabel" font-size={DIST_PX * kmPerPx}>{distanceKm} km</text>
  <!-- one ring, on the tuned repeater: a ring on every one hides the map -->
  {#if tuned}
    <circle cx={tuned.x} cy={tuned.y} r={reachKm} class="cover {hueOf(tuned)}" />
  {/if}
  {#each ordered as m (m.kind === 'one' ? m.p.id : m.points[0].id)}
    {#if m.kind === 'one'}
      {#if m.from}
        <line x1={m.from.x} y1={m.from.y} x2={m.x} y2={m.y} class="fan" />
      {/if}
      <g role="button" tabindex="0" aria-label={title(m.p)} onclick={() => pick(() => onTune(m.p.hz))} onkeydown={(e) => onKey(e, () => onTune(m.p.hz))} class="rep {hueOf(m.p)}" class:tuned={m.p.id === tunedId}>
        <title>{title(m.p)}</title>
        <circle cx={m.x} cy={m.y} r={Math.max(HIT_PX, radius(m) + 2) * kmPerPx} class="hit" />
        <circle cx={m.x} cy={m.y} r={radius(m) * kmPerPx} class="dot" />
      </g>
    {:else}
      {#if m.from}
        <line x1={m.from.x} y1={m.from.y} x2={m.x} y2={m.y} class="fan" />
      {/if}
      <g role="button" tabindex="0" aria-label={badgeTitle(m.points)} onclick={() => pick(() => open(m))} onkeydown={(e) => onKey(e, () => open(m))} class="badge {m.hue}">
        <title>{badgeTitle(m.points)}</title>
        <circle cx={m.x} cy={m.y} r={radius(m) * kmPerPx} class="dot" />
        <text x={m.x} y={m.y} dy="0.36em" font-size={11 * kmPerPx} class="count">{m.points.length}</text>
      </g>
    {/if}
  {/each}
  {#each towns as t (t.name)}
    {@const at = labels.get(`town:${t.name}`)}
    {#if at}
      <text x={at.x} y={at.y} class="town" font-size={TOWN_PX * kmPerPx}>{t.name}</text>
    {/if}
  {/each}
  {#each ordered as m (markId(m))}
    {@const at = labels.get(markId(m))}
    {#if at}
      <text x={at.x} y={at.y} font-size={LABEL_PX * kmPerPx} class="call {m.kind === 'one' ? hueOf(m.p) : m.hue}" class:tuned={m.kind === 'one' && m.p.id === tunedId}>{markWord(m)}</text>
    {/if}
  {/each}
  <circle r={DOT_PX * kmPerPx} class="site" />
  <text x={(DOT_PX + 4) * kmPerPx} y={4 * kmPerPx} class="you" font-size={YOU_PX * kmPerPx}>You</text>
</svg>
<!-- on-air rings sit above the drawing as HTML: WebKit runs their transform and opacity on the compositor, while a ring inside the SVG repaints the whole map every frame -->
{#if box}
  <div class="blips" aria-hidden="true">
    {#each ringing as p (p.id)}
      <span class="blip {hueOf(p)}" style:left="{box.width / 2 + (p.x - view.x) / kmPerPx}px" style:top="{box.height / 2 + (p.y - view.y) / kmPerPx}px"><span class="rings" style:transform="scale({ringReach(live.get(p.hz) ?? 0)})"><i></i><i></i></span></span>
    {/each}
  </div>
{/if}
<MapZoom {halfKm} limits={REP_LIMITS} onZoom={(f) => onView(zoomView(view, f))} />
<!-- outside the drawing -->
<span class="attrib">{ground ? 'Terrain: USGS · ' : ''}Natural Earth · US Census</span>

<style>
  svg {
    display: block;
    width: 100%;
    height: 100%;
    background: var(--bg-darker);
    cursor: grab;
    touch-action: none;
    user-select: none;
  }
  svg.dragging {
    cursor: grabbing;
  }
  .ground {
    fill: var(--bg-darker);
  }
  .road,
  .state {
    fill: none;
    vector-effect: non-scaling-stroke;
    stroke-linejoin: round;
    stroke-linecap: round;
    pointer-events: none;
  }
  .road.minor {
    stroke: var(--fg-dim);
    stroke-opacity: 0.28;
    stroke-width: 1;
  }
  .road.major {
    stroke: var(--fg-dim);
    stroke-opacity: 0.6;
    stroke-width: 1.8;
  }
  .state {
    stroke: var(--fg);
    stroke-opacity: 0.45;
    stroke-width: 1.2;
    stroke-dasharray: 5 4;
  }
  .attrib {
    position: absolute;
    right: 6px;
    top: 4px;
    font-size: 10px;
    color: var(--fg-dim);
    opacity: 0.7;
    pointer-events: none;
  }
  .distance {
    fill: none;
    stroke: var(--muted);
    stroke-width: 1;
    stroke-dasharray: 3 4;
    vector-effect: non-scaling-stroke;
    opacity: 0.6;
  }
  .distlabel {
    fill: var(--fg-dim);
  }
  /* a guess, and terrain decides more than distance here: barely there */
  .cover {
    fill: none;
    stroke-opacity: 0.35;
    stroke-width: 1;
    stroke-dasharray: 2 5;
    vector-effect: non-scaling-stroke;
    pointer-events: none;
  }
  .town {
    fill: var(--fg-dim);
    pointer-events: none;
  }
  .green {
    color: var(--green);
  }
  .blue {
    color: var(--blue);
  }
  .red {
    color: var(--red);
  }
  .cover {
    stroke: currentColor;
  }
  .rep,
  .badge {
    cursor: pointer;
    outline: none;
  }
  .hit {
    fill: transparent;
  }
  .dot {
    fill: currentColor;
    stroke: var(--bg-darker);
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
  }
  .rep:focus-visible .dot,
  .rep:hover .dot,
  .badge:focus-visible .dot,
  .badge:hover .dot {
    stroke: var(--fg-bright);
  }
  .fan {
    stroke: var(--fg-dim);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
    pointer-events: none;
  }
  .count {
    fill: var(--bg-darker);
    font-weight: 600;
    text-anchor: middle;
    pointer-events: none;
  }
  .call {
    fill: var(--fg);
    font-weight: 600;
    paint-order: stroke;
    stroke: var(--bg-darker);
    stroke-width: 3px;
    vector-effect: non-scaling-stroke;
    pointer-events: none;
  }
  .call.red {
    fill: var(--fg-dim);
  }
  .call.tuned {
    fill: var(--fg-bright);
  }
  /* a signal on the repeater's output: a steady ring around the dot, for the moments between the circles */
  .blips {
    position: absolute;
    inset: 0;
    pointer-events: none;
    overflow: hidden;
  }
  .blip {
    position: absolute;
    width: 0;
    height: 0;
  }
  /* circles leaving the repeater; they run only while it is on air, the one endless animation in the app (cost: CLAUDE.md) */
  .rings {
    position: absolute;
    inset: 0;
  }
  .blip i {
    position: absolute;
    left: 50%;
    top: 50%;
    box-sizing: border-box;
    width: 88px;
    height: 88px;
    margin: -44px 0 0 -44px;
    border: 3px solid currentColor;
    border-radius: 50%;
    opacity: 0;
    animation: ping 1.8s ease-out infinite;
  }
  .blip i:nth-child(2) {
    animation-delay: 0.9s;
  }
  @keyframes ping {
    from {
      transform: scale(0.14);
      opacity: 1;
    }
    to {
      transform: scale(1);
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .blip i {
      animation: none;
    }
  }
  .site {
    fill: var(--yellow);
    stroke: var(--bg-darker);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  .you {
    font-weight: 600;
    fill: var(--fg-bright);
  }
</style>
