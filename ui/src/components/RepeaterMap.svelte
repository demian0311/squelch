<script lang="ts">
  // Repeaters around the listener: a ring of likely reach on each, the tuned one in orange.
  import type { RepeaterPoint } from '../lib/repeaters';
  import { townLabels } from '../lib/repeaters';
  import { linePaths, type Frame, type Lines, type Palette, type Site } from '../lib/terrain';

  interface Props {
    points: RepeaterPoint[];
    reachKm: number;
    tunedHz: number | null;
    onTune: (hz: number) => void;
    site: Site;
    palette: Palette;
  }
  let { points, reachKm, tunedHz, onTune, site, palette }: Props = $props();

  // The viewBox is in km around the listener; `slice` fills the panel and crops east and west.
  const HALF_H_KM = 75;
  const HALF_W_KM = 200;
  const DISTANCES_KM = [25, 50, 100];
  let frame = $derived<Frame>({ site: { lat: site.lat, lon: site.lon }, halfWKm: HALF_W_KM, halfHKm: HALF_H_KM });

  // state lines and highways ship with the app; loaded on first use, then kept
  let lines = $state<Lines | null>(cachedLines);
  let paths = $derived(lines ? linePaths(lines, frame) : null);
  $effect(() => {
    if (!lines) loadLines().then((l) => (lines = l));
  });

  // shaded terrain: best effort, from cached or downloaded tiles; without them the lines draw alone
  let ground = $state<string | null>(null);
  $effect(() => {
    const key = `${frame.site.lat.toFixed(3)},${frame.site.lon.toFixed(3)},${Object.values(palette).join()}`;
    let stale = false;
    ground = groundCache.get(key) ?? null;
    if (!ground)
      drawGround(frame, palette).then((url) => {
        if (url) groundCache.set(key, url);
        if (!stale) ground = url;
      });
    return () => (stale = true);
  });

  let towns = $derived(townLabels(points, 8));
  let tuned = $derived(points.find((p) => p.hz === tunedHz));
  // the tuned repeater draws last, on top
  let ordered = $derived([...points].sort((a, b) => Number(a.hz === tunedHz) - Number(b.hz === tunedHz)));

  function onKey(e: KeyboardEvent, hz: number) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onTune(hz);
    }
  }
</script>

<script lang="ts" module>
  import * as api from '../lib/api';
  import { fromKm, mercator, shade, terrarium, tilesFor, TILE, type Lines as L, type Frame as F, type Palette as P } from '../lib/terrain';

  // elevation tiles at zoom 8 are about 470 m a pixel at 40° N; the ground image is drawn at 2 px a km
  const ZOOM = 8;
  const PX_PER_KM = 2;

  let cachedLines: L | null = null;
  async function loadLines(): Promise<L> {
    cachedLines ??= (await import('../lib/maplines.json')).default as unknown as L;
    return cachedLines;
  }
  const groundCache = new Map<string, string>();

  const loadImage = (src: string) =>
    new Promise<HTMLImageElement | null>((res) => {
      const i = new Image();
      i.onload = () => res(i);
      i.onerror = () => res(null);
      i.src = src;
    });

  /** The frame's shaded terrain as a PNG data URL, or null with no tiles at all. */
  async function drawGround(f: F, palette: P): Promise<string | null> {
    const tiles = tilesFor(f, ZOOM);
    const urls = await Promise.all(tiles.map((t) => api.terrainTile(ZOOM, t.x, t.y).catch(() => null)));
    if (urls.every((u) => !u)) return null;
    const x0 = Math.min(...tiles.map((t) => t.x));
    const y0 = Math.min(...tiles.map((t) => t.y));
    const mw = (Math.max(...tiles.map((t) => t.x)) - x0 + 1) * TILE;
    const mh = (Math.max(...tiles.map((t) => t.y)) - y0 + 1) * TILE;
    const mosaic = document.createElement('canvas');
    mosaic.width = mw;
    mosaic.height = mh;
    const mc = mosaic.getContext('2d', { willReadFrequently: true })!;
    const images = await Promise.all(urls.map((u) => (u ? loadImage(u) : null)));
    images.forEach((img, i) => img && mc.drawImage(img, (tiles[i].x - x0) * TILE, (tiles[i].y - y0) * TILE));
    const px = mc.getImageData(0, 0, mw, mh).data;

    const w = Math.round(2 * f.halfWKm * PX_PER_KM);
    const h = Math.round(2 * f.halfHKm * PX_PER_KM);
    const heights = new Float32Array(w * h);
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const [lon, lat] = fromKm(f.site, (x + 0.5) / PX_PER_KM - f.halfWKm, (y + 0.5) / PX_PER_KM - f.halfHKm);
        const [mx, my] = mercator(lon, lat, ZOOM);
        const ix = Math.floor(mx - x0 * TILE);
        const iy = Math.floor(my - y0 * TILE);
        const k = (iy * mw + ix) * 4;
        heights[y * w + x] = ix < 0 || iy < 0 || ix >= mw || iy >= mh || px[k + 3] === 0 ? NaN : terrarium(px[k], px[k + 1], px[k + 2]);
      }
    }
    const out = document.createElement('canvas');
    out.width = w;
    out.height = h;
    out.getContext('2d')!.putImageData(new ImageData(shade(heights, w, h, 1 / PX_PER_KM, palette), w, h), 0, 0);
    return out.toDataURL('image/png');
  }
</script>

<svg viewBox="{-HALF_W_KM} {-HALF_H_KM} {2 * HALF_W_KM} {2 * HALF_H_KM}" preserveAspectRatio="xMidYMid slice" role="group" aria-label="Repeaters around you">
  <rect x={-HALF_W_KM} y={-HALF_H_KM} width={2 * HALF_W_KM} height={2 * HALF_H_KM} class="ground" />
  {#if ground}
    <image href={ground} x={-HALF_W_KM} y={-HALF_H_KM} width={2 * HALF_W_KM} height={2 * HALF_H_KM} preserveAspectRatio="none" />
  {/if}
  {#if paths}
    <path d={paths.minor} class="road minor" />
    <path d={paths.major} class="road major" />
    <path d={paths.states} class="state" />
  {/if}
  {#each DISTANCES_KM as d}
    <circle r={d} class="distance" />
    <text x={d * 0.71 + 1.5} y={-d * 0.71 - 1.5} class="distlabel">{d} km</text>
  {/each}
  <!-- one ring, on the tuned repeater: a ring on every one hides the map -->
  {#if tuned}
    <circle cx={tuned.x} cy={tuned.y} r={reachKm} class="cover" />
  {/if}
  {#each towns as t (t.name)}
    <text x={t.x + 4} y={t.y - 3} class="town">{t.name}</text>
  {/each}
  {#each ordered as p (p.hz)}
    <g
      role="button"
      tabindex="0"
      aria-label="{p.call}, {p.city}, {(p.hz / 1e6).toFixed(4)} MHz, {Math.round(p.km)} km{p.digital ? `, may be ${p.digital} only` : ''}"
      onclick={() => onTune(p.hz)}
      onkeydown={(e) => onKey(e, p.hz)}
      class="rep"
      class:reach={p.reach}
      class:digital={p.digital}
      class:tuned={p.hz === tunedHz}
    >
      <title>{p.call} · {p.city} · {(p.hz / 1e6).toFixed(4)} MHz · {Math.round(p.km)} km{p.digital ? ` · may be ${p.digital} only` : ''}</title>
      <circle cx={p.x} cy={p.y} r="5" class="hit" />
      <circle cx={p.x} cy={p.y} r={p.hz === tunedHz ? 3.4 : 2.6} class="dot" />
    </g>
  {/each}
  <circle r="3" class="site" />
  <text x="5" y="1.8" class="you">You</text>
</svg>
<!-- outside the drawing, which `slice` crops at the sides -->
<span class="attrib">{ground ? 'Terrain: USGS · ' : ''}Natural Earth · US Census</span>

<style>
  svg {
    display: block;
    width: 100%;
    height: 100%;
    background: var(--bg-darker);
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
    font-size: 5px;
    fill: var(--fg-dim);
  }
  .cover {
    fill: var(--orange);
    fill-opacity: 0.1;
    stroke: var(--orange);
    stroke-opacity: 0.9;
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
    pointer-events: none;
  }
  .town {
    font-size: 6px;
    fill: var(--fg-dim);
    pointer-events: none;
  }
  .rep {
    cursor: pointer;
    outline: none;
  }
  .hit {
    fill: transparent;
  }
  .dot {
    fill: var(--bg-darker);
    stroke: var(--fg-dim);
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
  }
  .rep.reach .dot {
    fill: var(--accent);
    stroke: var(--accent);
  }
  /* may be digital only: hollow with a dashed edge */
  .rep.digital .dot {
    fill: var(--bg-darker);
    stroke-dasharray: 2 2;
  }
  .rep.tuned .dot {
    fill: var(--orange);
    stroke: var(--orange);
    stroke-dasharray: none;
  }
  .rep:focus-visible .dot,
  .rep:hover .dot {
    stroke: var(--fg-bright);
  }
  .site {
    fill: var(--yellow);
    stroke: var(--bg-darker);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  .you {
    font-size: 6px;
    font-weight: 600;
    fill: var(--fg-bright);
  }
</style>
