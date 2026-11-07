<script lang="ts">
  // Aircraft with a position, around the listener (or around themselves without a location): state lines,
  // highways, distance rings and an arrow per aircraft along its track. The map frames its planes by itself;
  // scroll, drag or the buttons make the view the user's until the fit button hands it back.
  import { heightColor, MAP_STEPS_KM, type MapPlane, type TrailPiece } from '../lib/adsb';
  import { mapGestures, ringKm, viewOf, zoomView, type MapLimits, type MapView } from '../lib/mapview';
  import MapZoom from './MapZoom.svelte';
  import { placeLabels } from '../lib/airplanes';
  import { GROUNDS, loadGround } from '../lib/ground';
  import { linePaths, type Frame, type Lines, type Palette, type Site } from '../lib/terrain';

  interface Props {
    planes: MapPlane[];
    centre: Site;
    /** half-height that holds every plane, km: the view while the user has not zoomed */
    halfKm: number;
    /** the listener sits at the centre */
    here: boolean;
    /** where aircraft have been, an SVG path in km per address */
    trails?: Record<string, string>;
    /** where aircraft have been, coloured by height and fading with age; takes the place of `trails` */
    tracks?: Record<string, TrailPiece[]>;
    selected?: string | null;
    onpick?: (icao: string) => void;
    /** an airport was clicked: its card opens */
    onairport?: (id: string) => void;
    /** a click on the map away from every plane */
    onclear?: () => void;
    airports?: { id: string; label: string; name: string; x: number; y: number; big: boolean; hit: boolean }[];
    /** a dashed line, plane to airport */
    link?: { x1: number; y1: number; x2: number; y2: number } | null;
    /** spots a message named */
    pins?: { x: number; y: number }[];
    /** shaded terrain under it all, in these colours */
    palette?: Palette;
  }
  let { planes, centre, halfKm: fitKm, here, trails = {}, tracks = {}, selected = null, onpick, onairport, onclear, airports = [], link = null, pins = [], palette }: Props = $props();

  // arrows and labels keep their size on screen whatever the panel's
  const ARROW_PX = 10;
  const LABEL_PX = 11;
  const SITE_PX = 4;
  const PICKED_SCALE = 1.6;
  // seen from above, nose up, about one unit from the middle to the nose
  const PLANE = 'M0 -1C.07 -1 .12 -.88 .12 -.72L.12 -.28 .95 .24 .95 .42 .12 .16 .1 .62 .38 .84 .38 .97 0 .86 -.38 .97 -.38 .84 -.1 .62 -.12 .16 -.95 .42 -.95 .24 -.12 -.28 -.12 -.72C-.12 -.88 -.07 -1 0 -1Z';
  let box = $state<DOMRectReadOnly | undefined>();

  // ---- zoom and pan, like a street map; null = the map frames its planes by itself ----
  const LIMITS: MapLimits = { nearestKm: 1.5, widestKm: MAP_STEPS_KM[MAP_STEPS_KM.length - 1] };
  let user = $state<MapView | null>(null);
  let view = $derived(user ?? viewOf(fitKm));
  const setView = (v: MapView) => (user = v);
  let halfKm = $derived(view.halfKm);
  let halfW = $derived(halfKm * (box?.width && box.height ? box.width / box.height : 1));
  let kmPerPx = $derived(box?.height ? (2 * halfKm) / box.height : 1);
  let svg = $state<SVGSVGElement>();
  let dragging = $state(false);
  const g = mapGestures({ view: () => view, kmPerPx: () => kmPerPx, svg: () => svg, limits: () => LIMITS, onView: setView, onDragging: (d) => (dragging = d) });
  // the click that ends a drag picks nothing
  const pick = (go: () => void) => {
    if (!g.dragged()) go();
  };
  // the rings: the widest step that fits, and half of it (while the map frames itself, the frame's own edge)
  let rings = $derived.by(() => {
    const d = ringKm(halfKm);
    return [d / 2, d];
  });

  // state lines and highways ship with the app; loaded on first use, then kept
  let lines = $state.raw<Lines | null>(cachedLines);
  $effect(() => {
    if (!lines) loadLines().then((l) => (lines = l));
  });
  // lines a little past the square, since `meet` leaves the wider side of the panel showing
  // the picked plane's name first, then the nearest: what has no room goes without. Symbols, the ring distances and
  // the attribution are in the way; with `meet` the visible top right corner is half the box out from the middle
  let labels = $derived.by(() => {
    const order = [...planes].sort((a, b) => Number(b.icao === selected) - Number(a.icao === selected) || Math.hypot(a.x, a.y) - Math.hypot(b.x, b.y));
    const spots = airports.map((a) => ({ id: `@${a.id}`, x: a.x, y: a.y, text: a.label, r: (a.big ? 6 : 4.5) + 1 }));
    const ringLabels = here ? rings.map((d) => ({ id: `#${d}`, x: d * 0.71 + 2 * kmPerPx, y: -d * 0.71 - (2 + LABEL_PX) * kmPerPx, text: `${d} km`, fixed: true })) : [];
    const corner = box ? [{ id: '#attrib', x: view.x + halfW - (6 + attrib.length * 5.4) * kmPerPx, y: view.y - halfKm + 4 * kmPerPx, text: attrib, fixed: true }] : [];
    const names = order.map((p) => ({ id: p.icao, x: p.x, y: p.y, text: p.name, r: (p.icao === selected ? PICKED_SCALE : 1) * ARROW_PX }));
    return placeLabels([...ringLabels, ...corner, ...spots, ...names], kmPerPx, LABEL_PX * 0.56, LABEL_PX * 1.15);
  });
  // the picked plane last, so it lies over the rest
  let drawn = $derived(selected ? [...planes].sort((a, b) => Number(a.icao === selected) - Number(b.icao === selected)) : planes);
  // these props come from one object made anew with each status; a derived number changes only when the number does,
  // so the lines are drawn again only when the map has moved
  let lat = $derived(centre.lat);
  let lon = $derived(centre.lon);
  // wider than tall, like the repeater map's, so a wide panel has ground to its edges; the smallest ground the view fits in
  const GROUND_RATIO = 8 / 3;
  let need = $derived(Math.max(Math.abs(view.y) + halfKm, (Math.abs(view.x) + halfW) / GROUND_RATIO));
  let level = $derived(GROUNDS.find((l) => l.halfHKm >= need) ?? GROUNDS[GROUNDS.length - 1]);
  let groundFrame = $derived<Frame>({ site: { lat, lon }, halfWKm: level.halfHKm * GROUND_RATIO, halfHKm: level.halfHKm });
  let paths = $derived(lines ? linePaths(lines, groundFrame) : null);
  let ground = $state<{ url: string; frame: Frame } | null>(null);
  let attrib = $derived(`${ground ? 'Terrain: USGS · ' : ''}Natural Earth · US Census${airports.length ? ' · OurAirports' : ''}`);
  $effect(() => {
    if (!palette) return;
    const f = groundFrame;
    let stale = false;
    loadGround(f, { ...palette }, level).then((url) => {
      if (!stale && url) ground = { url, frame: f };
    });
    return () => (stale = true);
  });
</script>

<script lang="ts" module>
  import type { Lines as L } from '../lib/terrain';
  let cachedLines: L | null = null;
  async function loadLines(): Promise<L> {
    cachedLines ??= (await import('../lib/maplines.json')).default as unknown as L;
    return cachedLines;
  }
</script>

<!-- the picked row's chevron also clears the pick, so the map needs no key handler -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<svg
  bind:this={svg}
  bind:contentRect={box}
  viewBox="{view.x - halfW} {view.y - halfKm} {2 * halfW} {2 * halfKm}"
  preserveAspectRatio="none"
  role="img"
  aria-label="{planes.length} aircraft on the map. Scroll to zoom, drag to move."
  class:dragging={dragging}
  onwheel={g.onwheel}
  onpointerdown={g.onpointerdown}
  onpointermove={g.onpointermove}
  onpointerup={g.onpointerup}
  onpointercancel={g.onpointerup}
  ondblclick={g.ondblclick}
  onclick={(e) => pick(() => { if (!(e.target as Element).closest('.plane')) onclear?.(); })}
>
  {#if ground}
    <image href={ground.url} x={-ground.frame.halfWKm} y={-ground.frame.halfHKm} width={2 * ground.frame.halfWKm} height={2 * ground.frame.halfHKm} preserveAspectRatio="none" />
  {/if}
  {#if paths}
    {#if palette}<path d={paths.minor} class="road minor" />{/if}
    <path d={paths.major} class="road" />
    <path d={paths.states} class="state" />
  {/if}
  {#if here}
    {#each rings as d (d)}
      <circle r={d} class="distance" />
      <text x={d * 0.71 + 2 * kmPerPx} y={-d * 0.71 - 2 * kmPerPx} font-size={LABEL_PX * 0.9 * kmPerPx} class="distlabel">{d} km</text>
    {/each}
    <circle r={SITE_PX * kmPerPx} class="site" />
  {/if}
  {#each Object.entries(tracks).sort(([a], [b]) => Number(a === selected) - Number(b === selected)) as [icao, pieces] (icao)}
    <g class:dim={selected && icao !== selected}>
      {#each pieces as t, k (k)}
        <path d={t.d} class="track" class:picked={icao === selected} style="stroke:{t.color};opacity:{t.opacity}" />
      {/each}
    </g>
  {/each}
  {#each Object.entries(trails) as [icao, d] (icao)}
    <path {d} class="trail" class:picked={icao === selected} />
  {/each}
  {#if link}<line {...link} class="link" />{/if}
  {#each pins as p (`${p.x},${p.y}`)}
    <circle cx={p.x} cy={p.y} r={4 * kmPerPx} class="pin" />
  {/each}
  {#each airports as a (a.id)}
    {@const l = labels.get(`@${a.id}`)}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <g class="airport" class:hit={a.hit} class:big={a.big} class:clickable={!!onairport} onclick={(e) => { e.stopPropagation(); pick(() => onairport?.(a.id)); }}>
      <title>{a.name}: click for its channels</title>
      <!-- a ring crossed by a runway, the way aviation charts draw an airport -->
      <g transform="translate({a.x} {a.y}) scale({(a.big ? 6 : 4.5) * kmPerPx})">
        <circle r="1" />
        <line x1="-0.7" y1="0.7" x2="0.7" y2="-0.7" />
      </g>
      {#if l}<text x={l.x} y={l.y} font-size={LABEL_PX * 0.9 * kmPerPx}>{a.label}</text>{/if}
    </g>
  {/each}
  {#each drawn as p (p.icao)}
    {@const l = labels.get(p.icao)}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <g transform="translate({p.x} {p.y})" class="plane" class:stale={p.stale} class:alarm={p.alarm} class:picked={p.icao === selected} class:dim={selected && p.icao !== selected} style="--height:{heightColor(p.alt_ft)}" onclick={() => pick(() => onpick?.(p.icao))}>
      <title>{p.name}</title>
      {#if p.heading !== null}
        <path d={PLANE} transform="rotate({p.heading}) scale({(p.icao === selected ? PICKED_SCALE : 1) * ARROW_PX * kmPerPx})" class="arrow" />
      {:else}
        <circle r={ARROW_PX * 0.5 * kmPerPx} class="arrow" />
      {/if}
      {#if l}<text x={l.x - p.x} y={l.y - p.y} font-size={LABEL_PX * kmPerPx} class="label">{p.name}</text>{/if}
    </g>
  {/each}
</svg>
<MapZoom {halfKm} limits={LIMITS} onZoom={(f) => setView(zoomView(view, f, LIMITS))} fit={{ on: !user, title: 'Fit every plane: the map follows them again', onFit: () => (user = null) }} />
<span class="attrib">{attrib}</span>

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
  .airport circle {
    fill: var(--bg-darker);
    stroke: var(--fg-dim);
    stroke-width: 1.2;
    vector-effect: non-scaling-stroke;
  }
  .airport line {
    stroke: var(--fg-dim);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  .airport text {
    fill: var(--fg-dim);
    paint-order: stroke;
    stroke: var(--bg-darker);
    stroke-width: 3;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
  }
  .airport.big circle,
  .airport.big line {
    stroke: var(--fg-bright);
  }
  .airport.big text {
    fill: var(--fg-bright);
  }
  .airport.hit circle,
  .airport.hit line {
    stroke: var(--yellow);
  }
  .airport.hit text {
    fill: var(--yellow);
    font-weight: 600;
  }
  .link {
    stroke: var(--yellow);
    stroke-width: 1.5;
    stroke-dasharray: 5 4;
    vector-effect: non-scaling-stroke;
  }
  .pin {
    fill: var(--orange);
    stroke: var(--bg-darker);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .road.minor {
    stroke-opacity: 0.25;
    stroke-width: 1;
  }
  .road,
  .state {
    fill: none;
    vector-effect: non-scaling-stroke;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .road {
    stroke: var(--fg-dim);
    stroke-opacity: 0.4;
    stroke-width: 1.2;
  }
  .state {
    stroke: var(--fg);
    stroke-opacity: 0.45;
    stroke-width: 1.2;
    stroke-dasharray: 5 4;
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
  .site {
    fill: var(--yellow);
    stroke: var(--bg-darker);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .track {
    fill: none;
    stroke-width: 1.4;
    stroke-linejoin: round;
    stroke-linecap: round;
    vector-effect: non-scaling-stroke;
  }
  .track.picked {
    stroke-width: 2.5;
  }
  .trail {
    fill: none;
    stroke: var(--accent);
    stroke-opacity: 0.45;
    stroke-width: 1.2;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
  }
  .trail.picked {
    stroke-opacity: 1;
    stroke-width: 2;
  }
  .plane {
    cursor: pointer;
  }
  .arrow {
    fill: var(--height, var(--accent));
    stroke: var(--bg-darker);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .alarm .arrow {
    fill: var(--red);
  }
  .picked .arrow {
    fill: var(--yellow);
    stroke: var(--fg-bright);
  }
  .picked .label {
    font-weight: 700;
  }
  .dim {
    opacity: 0.25;
  }
  .label {
    fill: var(--fg-bright);
    paint-order: stroke;
    stroke: var(--bg-darker);
    stroke-width: 3;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
  }
  .stale:not(.picked):not(.dim) {
    opacity: 0.45;
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
</style>
