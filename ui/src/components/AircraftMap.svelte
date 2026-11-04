<script lang="ts">
  // Aircraft with a position, around the listener (or around themselves without a location): state lines,
  // highways, distance rings and an arrow per aircraft along its track.
  import type { MapPlane } from '../lib/adsb';
  import { linePaths, type Lines, type Site } from '../lib/terrain';

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
  }
  let { planes, centre, halfKm, here, trails = {}, selected = null, onpick }: Props = $props();

  // arrows and labels keep their size on screen whatever the panel's
  const ARROW_PX = 7;
  const LABEL_PX = 11;
  const SITE_PX = 4;
  let box = $state<DOMRectReadOnly | undefined>();
  let kmPerPx = $derived(box?.width && box.height ? (2 * halfKm) / Math.min(box.width, box.height) : 1);

  // state lines and highways ship with the app; loaded on first use, then kept
  let lines = $state.raw<Lines | null>(cachedLines);
  $effect(() => {
    if (!lines) loadLines().then((l) => (lines = l));
  });
  // lines a little past the square, since `meet` leaves the wider side of the panel showing
  // these props come from one object made anew with each status; a derived number changes only when the number does,
  // so the lines are drawn again only when the map has moved
  let lat = $derived(centre.lat);
  let lon = $derived(centre.lon);
  let half = $derived(halfKm * 2);
  let paths = $derived(lines ? linePaths(lines, { site: { lat, lon }, halfWKm: half, halfHKm: half }) : null);
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
  {#if paths}
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
  {#each Object.entries(trails) as [icao, d] (icao)}
    <path {d} class="trail" class:picked={icao === selected} />
  {/each}
  {#each planes as p (p.icao)}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <g transform="translate({p.x} {p.y})" class="plane" class:stale={p.stale} class:alarm={p.alarm} class:picked={p.icao === selected} onclick={() => onpick?.(p.icao)}>
      <title>{p.name}</title>
      {#if p.heading !== null}
        <path d="M0 -1L0.7 0.8L0 0.4L-0.7 0.8Z" transform="rotate({p.heading}) scale({ARROW_PX * kmPerPx})" class="arrow" />
      {:else}
        <circle r={ARROW_PX * 0.5 * kmPerPx} class="arrow" />
      {/if}
      <text x={ARROW_PX * 1.2 * kmPerPx} y={LABEL_PX * 0.35 * kmPerPx} font-size={LABEL_PX * kmPerPx} class="label">{p.name}</text>
    </g>
  {/each}
</svg>
<span class="attrib">Natural Earth · US Census</span>

<style>
  svg {
    display: block;
    width: 100%;
    height: 100%;
    background: var(--bg-darker);
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
