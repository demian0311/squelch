<script lang="ts">
  // Where the space station is: world map, ground track, the area that can hear it, and the listener.
  import { geoCircle, geoEquirectangular, geoPath } from 'd3-geo';
  import { feature } from 'topojson-client';
  import type { GeometryCollection, Topology } from 'topojson-specification';
  import land110 from 'world-atlas/land-110m.json';
  import type { GroundPoint } from '../lib/api';
  import { footprintDeg, groundAt, mapView } from '../lib/iss';

  interface Props {
    track: GroundPoint[];
    now: number;
    site: { lat: number; lon: number } | null;
    /** Zoom to the listener; otherwise the whole world. */
    local: boolean;
  }
  let { track, now, site, local }: Props = $props();

  // x = lon + 180, y = 90 − lat: one unit per degree, so the viewBox is in degrees.
  const path = geoPath(geoEquirectangular().scale(180 / Math.PI).translate([180, 90]).precision(0.1));
  const topo = land110 as unknown as Topology<{ land: GeometryCollection }>;
  const land = path(feature(topo, topo.objects.land)) ?? '';
  const graticule = Array.from({ length: 11 }, (_, i) => `M0 ${i * 15 + 15}H360`).join('') + Array.from({ length: 23 }, (_, i) => `M${i * 15 + 15} 0V180`).join('');

  let here = $derived(groundAt(track, now));
  let line = (pts: GroundPoint[]) => (pts.length > 1 ? (path({ type: 'LineString', coordinates: pts.map((p) => [p.lon, p.lat]) }) ?? '') : '');
  let past = $derived(line([...track.filter((p) => p.t <= now), ...(here ? [here] : [])]));
  let ahead = $derived(line([...(here ? [here] : []), ...track.filter((p) => p.t > now)]));
  let reach = $derived(here ? (path(geoCircle().center([here.lon, here.lat]).radius(footprintDeg(here.alt_km))()) ?? '') : '');
  let view = $derived(mapView(site, local));
  // Marker sizes in degrees, smaller when zoomed in.
  let k = $derived(local ? 0.4 : 1);
</script>

<svg viewBox={view} preserveAspectRatio="xMidYMid meet" role="img" aria-label={here ? `Space station over ${here.lat.toFixed(0)}°, ${here.lon.toFixed(0)}°` : 'Space station position unknown'}>
  <rect x="0" y="0" width="360" height="180" class="sea" />
  <path d={graticule} class="grid" />
  <path d={land} class="land" />
  {#if reach}<path d={reach} class="reach" />{/if}
  <path d={past} class="past" />
  <path d={ahead} class="ahead" />
  {#if site}
    <circle cx={site.lon + 180} cy={90 - site.lat} r={2.2 * k} class="site" />
  {/if}
  {#if here}
    <circle cx={here.lon + 180} cy={90 - here.lat} r={2.6 * k} class="iss" />
    <text x={here.lon + 180 + 3 * k} y={90 - here.lat + 1.2 * k} font-size={4 * k} class="label">ISS</text>
  {/if}
</svg>

<style>
  svg {
    display: block;
    width: 100%;
    height: 100%;
    background: var(--bg-darker);
  }
  .sea {
    fill: var(--bg-darker);
  }
  .grid {
    stroke: var(--muted);
    stroke-width: 0.15;
    vector-effect: non-scaling-stroke;
    opacity: 0.5;
  }
  .land {
    fill: var(--bg-light);
    stroke: var(--muted);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .reach {
    fill: var(--green);
    fill-opacity: 0.12;
    stroke: var(--green);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .past,
  .ahead {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  .ahead {
    stroke-dasharray: 6 4;
  }
  .past {
    opacity: 0.45;
  }
  .site {
    fill: var(--yellow);
    stroke: var(--bg-darker);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .iss {
    fill: var(--green);
    stroke: var(--bg-darker);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  .label {
    fill: var(--fg-bright);
    font-weight: 600;
  }
</style>
