<script lang="ts">
  // Repeaters around the listener: a ring of likely reach on each, the tuned one in orange.
  import type { RepeaterPoint } from '../lib/repeaters';
  import { townLabels } from '../lib/repeaters';

  interface Props {
    points: RepeaterPoint[];
    reachKm: number;
    tunedHz: number | null;
    onTune: (hz: number) => void;
  }
  let { points, reachKm, tunedHz, onTune }: Props = $props();

  // The viewBox is in km around the listener; `slice` fills the panel and crops east and west.
  const HALF_H_KM = 75;
  const HALF_W_KM = 200;
  const DISTANCES_KM = [25, 50, 100];

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

<svg viewBox="{-HALF_W_KM} {-HALF_H_KM} {2 * HALF_W_KM} {2 * HALF_H_KM}" preserveAspectRatio="xMidYMid slice" role="group" aria-label="Repeaters around you">
  <rect x={-HALF_W_KM} y={-HALF_H_KM} width={2 * HALF_W_KM} height={2 * HALF_H_KM} class="ground" />
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
