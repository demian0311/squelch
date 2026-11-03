<script lang="ts">
  // Aircraft with a position, around the listener (or around themselves without a location): state lines,
  // highways, distance rings and an arrow per aircraft along its track.
  import type { MapPlane } from '../lib/adsb';
  import { placeLabels } from '../lib/airplanes';
  import { GROUNDS, loadGround } from '../lib/ground';
  import { linePaths, type Frame, type Lines, type Palette, type Site } from '../lib/terrain';

  interface Props {
    planes: MapPlane[];
    centre: Site;
    halfKm: number;
    /** the listener sits at the centre */
    here: boolean;
    /** where aircraft have been, an SVG path in km per address */
    trails?: Record<string, string>;
    selected?: string | null;
    onpick?: (icao: string) => void;
    airports?: { id: string; label: string; name: string; x: number; y: number; big: boolean; hit: boolean }[];
    /** a dashed line, plane to airport */
    link?: { x1: number; y1: number; x2: number; y2: number } | null;
    /** spots a message named */
    pins?: { x: number; y: number }[];
    /** shaded terrain under it all, in these colours */
    palette?: Palette;
  }
  let { planes, centre, halfKm, here, trails = {}, selected = null, onpick, airports = [], link = null, pins = [], palette }: Props = $props();

  // arrows and labels keep their size on screen whatever the panel's
  const ARROW_PX = 7;
  const LABEL_PX = 11;
  const SITE_PX = 4;
  let box = $state<DOMRectReadOnly | undefined>();
  let kmPerPx = $derived(box?.width && box.height ? (2 * halfKm) / Math.min(box.width, box.height) : 1);

  // state lines and highways ship with the app; loaded on first use, then kept
  let lines = $state<Lines | null>(cachedLines);
  $effect(() => {
    if (!lines) loadLines().then((l) => (lines = l));
  });
  // lines a little past the square, since `meet` leaves the wider side of the panel showing
  // the picked plane's name first, then the nearest: what has no room goes without
  let labels = $derived.by(() => {
    const order = [...planes].sort((a, b) => Number(b.icao === selected) - Number(a.icao === selected) || Math.hypot(a.x, a.y) - Math.hypot(b.x, b.y));
    const spots = airports.map((a) => ({ id: `@${a.id}`, x: a.x, y: a.y, text: a.label }));
    return placeLabels([...spots, ...order.map((p) => ({ id: p.icao, x: p.x, y: p.y, text: p.name }))], kmPerPx, LABEL_PX * 0.56, LABEL_PX * 1.15, ARROW_PX * 1.1);
  });
  let paths = $derived(lines ? linePaths(lines, { site: centre, halfWKm: halfKm * 2, halfHKm: halfKm * 2 }) : null);

  // wider than tall, like the repeater map's, so a wide panel has ground to its edges
  const GROUND_RATIO = 8 / 3;
  let level = $derived(GROUNDS.find((g) => g.halfHKm >= halfKm) ?? GROUNDS[GROUNDS.length - 1]);
  let groundFrame = $derived<Frame>({ site: { lat: centre.lat, lon: centre.lon }, halfWKm: level.halfHKm * GROUND_RATIO, halfHKm: level.halfHKm });
  let ground = $state<{ url: string; frame: Frame } | null>(null);
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

<svg bind:contentRect={box} viewBox="{-halfKm} {-halfKm} {2 * halfKm} {2 * halfKm}" preserveAspectRatio="xMidYMid meet" role="img" aria-label="{planes.length} aircraft on the map">
  {#if ground}
    <image href={ground.url} x={-ground.frame.halfWKm} y={-ground.frame.halfHKm} width={2 * ground.frame.halfWKm} height={2 * ground.frame.halfHKm} preserveAspectRatio="none" />
  {/if}
  {#if paths}
    {#if palette}<path d={paths.minor} class="road minor" />{/if}
    <path d={paths.major} class="road" />
    <path d={paths.states} class="state" />
  {/if}
  {#if here}
    {#each [halfKm / 2, halfKm] as d (d)}
      <circle r={d} class="distance" />
      <text x={d * 0.71 + 2 * kmPerPx} y={-d * 0.71 - 2 * kmPerPx} font-size={LABEL_PX * 0.9 * kmPerPx} class="distlabel">{d} km</text>
    {/each}
    <circle r={SITE_PX * kmPerPx} class="site" />
  {/if}
  {#each airports as a (a.id)}
    {@const l = labels.get(`@${a.id}`)}
    <g class="airport" class:hit={a.hit} class:big={a.big}>
      <title>{a.name}</title>
      <rect x={a.x - 2.5 * kmPerPx} y={a.y - 2.5 * kmPerPx} width={5 * kmPerPx} height={5 * kmPerPx} />
      {#if l}<text x={l.x} y={l.y} font-size={LABEL_PX * 0.9 * kmPerPx}>{a.label}</text>{/if}
    </g>
  {/each}
  {#each Object.entries(trails) as [icao, d] (icao)}
    <path {d} class="trail" class:picked={icao === selected} />
  {/each}
  {#if link}<line {...link} class="link" />{/if}
  {#each pins as p (`${p.x},${p.y}`)}
    <circle cx={p.x} cy={p.y} r={4 * kmPerPx} class="pin" />
  {/each}
  {#each planes as p (p.icao)}
    {@const l = labels.get(p.icao)}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <g transform="translate({p.x} {p.y})" class="plane" class:stale={p.stale} class:alarm={p.alarm} class:picked={p.icao === selected} onclick={() => onpick?.(p.icao)}>
      <title>{p.name}</title>
      {#if p.heading !== null}
        <path d="M0 -1L0.7 0.8L0 0.4L-0.7 0.8Z" transform="rotate({p.heading}) scale({ARROW_PX * kmPerPx})" class="arrow" />
      {:else}
        <circle r={ARROW_PX * 0.5 * kmPerPx} class="arrow" />
      {/if}
      {#if l}<text x={l.x - p.x} y={l.y - p.y} font-size={LABEL_PX * kmPerPx} class="label">{p.name}</text>{/if}
    </g>
  {/each}
</svg>
<span class="attrib">{ground ? 'Terrain: USGS · ' : ''}Natural Earth · US Census{#if airports.length} · OurAirports{/if}</span>

<style>
  svg {
    display: block;
    width: 100%;
    height: 100%;
    background: var(--bg-darker);
  }
  .airport rect {
    fill: var(--fg-dim);
  }
  .airport text {
    fill: var(--fg-dim);
  }
  .airport.big rect {
    fill: var(--fg);
  }
  .airport.hit rect {
    fill: var(--yellow);
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
    fill: var(--accent);
    stroke: var(--bg-darker);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .alarm .arrow {
    fill: var(--red);
  }
  .picked .arrow {
    fill: var(--yellow);
  }
  .label {
    fill: var(--fg-bright);
  }
  .stale {
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
