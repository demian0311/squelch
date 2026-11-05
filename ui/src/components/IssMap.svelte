<script lang="ts">
  // Where the space station is: world map, ground track, the area that can hear it, and the listener.
  import { geoCircle, geoEquirectangular, geoPath } from 'd3-geo';
  import { feature } from 'topojson-client';
  import type { GeometryCollection, Topology } from 'topojson-specification';
  import land110 from 'world-atlas/land-110m.json';
  import type { GroundPoint, IssPass } from '../lib/api';
  import { clock, footprintDeg, groundAt, mapView, passColour } from '../lib/iss';

  interface Props {
    track: GroundPoint[];
    now: number;
    site: { lat: number; lon: number } | null;
    /** Zoom to the listener; otherwise the whole world. */
    local: boolean;
    /** A pass to draw instead of the live position: its track, the station at its peak and the ground that hears it. */
    pass?: IssPass | null;
  }
  let { track, now, site, local, pass = null }: Props = $props();

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
  // The hovered pass: the ground the station covers at its peak, the part of the track above the horizon, and the rest around it.
  let peak = $derived(pass ? groundAt(pass.track, pass.top) : null);
  let passReach = $derived(peak ? (path(geoCircle().center([peak.lon, peak.lat]).radius(footprintDeg(peak.alt_km))()) ?? '') : '');
  let passUp = $derived(pass ? line(pass.track.filter((p) => p.t >= pass.rise && p.t <= pass.set)) : '');
  let passAround = $derived(pass ? line(pass.track) : '');
  let passEnds = $derived(pass ? [groundAt(pass.track, pass.rise), groundAt(pass.track, pass.set)] : []);
  let colour = $derived(pass ? passColour(pass.max_elevation) : '');
  let zoomed = $derived(local || pass !== null);
  let view = $derived(mapView(site, zoomed));
  // Marker sizes in degrees, smaller when zoomed in.
  let k = $derived(zoomed ? 0.4 : 1);
</script>

<svg viewBox={view} preserveAspectRatio="xMidYMid meet" role="img" aria-label={here ? `Space station over ${here.lat.toFixed(0)}°, ${here.lon.toFixed(0)}°` : 'Space station position unknown'}>
  <rect x="0" y="0" width="360" height="180" class="sea" />
  <path d={graticule} class="grid" />
  <path d={land} class="land" />
  <g class="live" class:faded={pass !== null}>
    {#if reach}<path d={reach} class="reach" />{/if}
    <path d={past} class="past" />
    <path d={ahead} class="ahead" />
    {#if here}
      <circle cx={here.lon + 180} cy={90 - here.lat} r={2.6 * k} class="iss" />
      <text x={here.lon + 180 + 3 * k} y={90 - here.lat + 1.2 * k} font-size={4 * k} class="label">ISS</text>
    {/if}
  </g>
  {#if pass && peak}
    <g style:--pass={colour}>
      <path d={passReach} class="passreach" />
      <path d={passAround} class="passaround" />
      <path d={passUp} class="passup" />
      <circle cx={peak.lon + 180} cy={90 - peak.lat} r={1.6 * k} class="peak" />
      {#each passEnds as e, i (i)}
        {#if e}<text x={e.lon + 180 + 1.2 * k} y={90 - e.lat + (i ? -1 : 2.5) * k} font-size={2.6 * k} class="end">{i ? 'sets' : 'rises'} {clock(i ? pass.set : pass.rise)}</text>{/if}
      {/each}
    </g>
  {/if}
  {#if site}
    <circle cx={site.lon + 180} cy={90 - site.lat} r={2.2 * k} class="site" />
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
  .live.faded {
    opacity: 0.3;
  }
  .passreach {
    fill: var(--pass);
    fill-opacity: 0.14;
    stroke: var(--pass);
    stroke-opacity: 0.7;
    stroke-width: 0.6;
    vector-effect: non-scaling-stroke;
  }
  .passaround {
    fill: none;
    stroke: var(--pass);
    stroke-opacity: 0.4;
    stroke-width: 1;
    stroke-dasharray: 1.5 1.5;
    vector-effect: non-scaling-stroke;
  }
  .passup {
    fill: none;
    stroke: var(--pass);
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
  }
  .peak {
    fill: var(--pass);
    stroke: var(--bg-darker);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  .end {
    fill: var(--fg-dim);
  }
</style>
