<script lang="ts">
  // Repeaters around the listener: a ring of likely reach on each, the tuned one in orange.
  import { untrack } from 'svelte';
  import type { RepeaterPoint } from '../lib/repeaters';
  import { RANGES_KM, townLabels } from '../lib/repeaters';
  import { groundKey, linePaths, type Frame, type Lines, type Palette, type Site } from '../lib/terrain';

  interface Props {
    points: RepeaterPoint[];
    reachKm: number;
    tunedId: string | null;
    /** output frequencies with a signal on them now, and how strong */
    live: Map<number, number>;
    onTune: (hz: number) => void;
    /** km shown from the listener to the top and bottom of the widest panel */
    range: number;
    onRange: (km: number) => void;
    site: Site;
    palette: Palette;
  }
  let { points, reachKm, tunedId, live, onTune, range, onRange, site, palette }: Props = $props();

  // The viewBox is in km around the listener; `slice` fills the panel and crops what does not fit.
  const WIDTH_RATIO = 8 / 3;
  // the largest ring that fits the range
  const DISTANCES_KM = [25, 50, 100, 200, 300];
  // dots and words keep their size on screen whatever the panel's
  const DOT_PX = 3;
  const TUNED_PX = 4;
  const HIT_PX = 9;
  const TOWN_PX = 12;
  const YOU_PX = 13;
  const DIST_PX = 10;
  let halfH = $derived(range);
  let halfW = $derived(range * WIDTH_RATIO);
  let box = $state<DOMRectReadOnly | undefined>();
  let kmPerPx = $derived(box?.width ? 1 / Math.max(box.width / (2 * halfW), box.height / (2 * halfH)) : 1);
  let distanceKm = $derived(DISTANCES_KM.filter((d) => d <= range).at(-1) ?? range);
  // the coarsest ground that covers the range changes only at its stops, so most slider moves redraw nothing
  let level = $derived(GROUNDS.find((g) => g.halfHKm >= range) ?? GROUNDS[GROUNDS.length - 1]);
  let frame = $derived<Frame>({ site: { lat: site.lat, lon: site.lon }, halfWKm: level.halfHKm * WIDTH_RATIO, halfHKm: level.halfHKm });

  // state lines and highways ship with the app; loaded on first use, then kept
  let lines = $state<Lines | null>(cachedLines);
  let paths = $derived(lines ? linePaths(lines, frame) : null);
  $effect(() => {
    if (!lines) loadLines().then((l) => (lines = l));
  });

  // shaded terrain: best effort. Memory, then the disk cache, then drawn from tiles and saved; without tiles the lines draw alone
  let ground = $state<{ url: string; frame: Frame } | null>(null);
  $effect(() => {
    const f = frame;
    const l = level;
    const p = { ...palette };
    const key = groundKey(f, p);
    let stale = false;
    const cached = groundCache.get(key);
    // moving the slider keeps the old ground until the new one is ready; another place must not
    const old = untrack(() => ground);
    if (cached) ground = { url: cached, frame: f };
    else if (old && (old.frame.site.lat !== f.site.lat || old.frame.site.lon !== f.site.lon)) ground = null;
    if (!cached)
      (async () => {
        let url = await api.ground(key).catch(() => null);
        if (!url) {
          url = await drawGround(f, p, l);
          if (url) api.saveGround(key, url).catch(() => {});
        }
        if (url) groundCache.set(key, url);
        if (!stale && url) ground = { url, frame: f };
      })();
    return () => (stale = true);
  });

  // what the panel's shape leaves on screen
  let inView = $derived(box?.width ? points.filter((p) => Math.abs(p.x) <= (box!.width / 2) * kmPerPx && Math.abs(p.y) <= (box!.height / 2) * kmPerPx) : points);
  let towns = $derived(townLabels(inView, 8));
  let tuned = $derived(points.find((p) => p.id === tunedId));
  // the tuned repeater draws last, on top
  let ordered = $derived([...inView].sort((a, b) => Number(a.id === tunedId) - Number(b.id === tunedId)));
  // a shared frequency: what is on the air is taken to be the nearest one, and `points` is nearest first
  let onAir = $derived.by(() => {
    const seen = new Set<number>();
    return points.filter((p) => live.has(p.hz) && !seen.has(p.hz) && seen.add(p.hz));
  });
  let rangeAt = $derived(Math.max(0, RANGES_KM.indexOf(range)));
  const place = (p: RepeaterPoint) => `${p.call} · ${p.city} · ${(p.hz / 1e6).toFixed(4)} MHz · ${Math.round(p.km)} km`;
  const onAirNote = (p: RepeaterPoint) => (onAir.includes(p) ? ` · on air, ${live.get(p.hz)!.toFixed(0)} dB above the noise` : '');

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

  // elevation tiles at zoom 8 are about 470 m a pixel at 40° N, so each wider ground takes tiles one zoom out and half the pixels a km
  const GROUNDS = [
    { halfHKm: 75, zoom: 8, pxPerKm: 2 },
    { halfHKm: 150, zoom: 7, pxPerKm: 1 },
    { halfHKm: 300, zoom: 6, pxPerKm: 0.5 },
  ];

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
  async function drawGround(f: F, palette: P, { zoom: ZOOM, pxPerKm: PX_PER_KM }: (typeof GROUNDS)[number]): Promise<string | null> {
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

<svg bind:contentRect={box} viewBox="{-halfW} {-halfH} {2 * halfW} {2 * halfH}" preserveAspectRatio="xMidYMid slice" role="group" aria-label="Repeaters around you. Each one tunes when pressed.">
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
    <circle cx={tuned.x} cy={tuned.y} r={reachKm} class="cover" />
  {/if}
  {#each towns as t (t.name)}
    <text x={t.x + 6 * kmPerPx} y={t.y - 4 * kmPerPx} class="town" font-size={TOWN_PX * kmPerPx}>{t.name}</text>
  {/each}
  {#each ordered as p (p.id)}
    <g
      role="button"
      tabindex="0"
      aria-label="{place(p)}{onAirNote(p)}{p.digital ? `, may be ${p.digital} only` : ''}"
      onclick={() => onTune(p.hz)}
      onkeydown={(e) => onKey(e, p.hz)}
      class="rep"
      class:reach={p.reach}
      class:digital={p.digital}
      class:tuned={p.id === tunedId}
    >
      <title>{place(p)}{onAirNote(p)}{p.digital ? ` · may be ${p.digital} only` : ''}</title>
      <circle cx={p.x} cy={p.y} r={HIT_PX * kmPerPx} class="hit" />
      <circle cx={p.x} cy={p.y} r={(p.id === tunedId ? TUNED_PX : DOT_PX) * kmPerPx} class="dot" />
    </g>
  {/each}
  <circle r={TUNED_PX * kmPerPx} class="site" />
  <text x={(TUNED_PX + 4) * kmPerPx} y={4 * kmPerPx} class="you" font-size={YOU_PX * kmPerPx}>You</text>
</svg>
<!-- on-air rings sit above the drawing as HTML: WebKit runs their transform and opacity on the compositor, while a ring inside the SVG repaints the whole map every frame -->
{#if box}
  <div class="blips" aria-hidden="true">
    {#each onAir as p (p.id)}
      <span class="blip" class:tuned={p.id === tunedId} style:left="{box.width / 2 + p.x / kmPerPx}px" style:top="{box.height / 2 + p.y / kmPerPx}px"><b>{p.call}</b></span>
    {/each}
  </div>
{/if}
<label class="range" title="How far the map shows around you">
  <input type="range" min="0" max={RANGES_KM.length - 1} step="1" value={rangeAt} aria-label="Map range" aria-valuetext="{range} km" oninput={(e) => onRange(RANGES_KM[+e.currentTarget.value])} />
  <span>{range} km</span>
</label>
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
  /* a signal on the repeater's output: a ring around the dot. Steady, not pulsing: a running animation redraws the whole window 60 times a second here */
  .blips {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .blip {
    position: absolute;
    width: 12px;
    height: 12px;
    margin: -6px 0 0 -6px;
    box-sizing: border-box;
    border: 1.5px solid var(--green);
    border-radius: 50%;
    opacity: 0.8;
  }
  .blip.tuned {
    border-color: var(--orange);
  }
  .blip b {
    position: absolute;
    left: 13px;
    top: -4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--fg-bright);
    white-space: nowrap;
  }
  .range {
    position: absolute;
    left: 8px;
    top: 6px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    color: var(--fg);
    background: color-mix(in srgb, var(--bg-darker) 80%, transparent);
  }
  .range input {
    width: 80px;
    margin: 0;
    accent-color: var(--accent);
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
