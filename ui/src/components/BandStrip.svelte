<script lang="ts">
  // The whole band in one bar: remembered stations as ticks, the waterfall's view as a box.
  // Click a tick to tune it, anywhere else to move the view there; drag the box to pan.
  import { pickTick, stripLabels, type Tick } from '../lib/stations';

  let {
    lo,
    hi,
    vs,
    ve,
    tuned,
    ticks,
    hue,
    onTune,
    onView,
    onGlide,
  }: {
    lo: number;
    hi: number;
    vs: number;
    ve: number;
    tuned: number;
    ticks: Tick[];
    hue: string;
    onTune: (hz: number) => void;
    /** Follows a drag. */
    onView: (vs: number, ve: number) => void;
    /** Eases there, after a click. */
    onGlide: (vs: number, ve: number) => void;
  } = $props();

  /** The view box stays grabbable on a band many screens wide. */
  const MIN_BOX_PX = 10;

  let el: HTMLDivElement;
  let width = $state(1000);
  let drag: { x: number; vs: number; ve: number; box: boolean; moved: boolean } | null = null;

  const x = (hz: number) => ((hz - lo) / (hi - lo)) * 100;
  let boxW = $derived(Math.max(((ve - vs) / (hi - lo)) * width, MIN_BOX_PX));
  let labels = $derived(stripLabels(lo, hi, width));

  function hzAt(clientX: number) {
    const r = el.getBoundingClientRect();
    return lo + ((clientX - r.left) / r.width) * (hi - lo);
  }
  function down(e: PointerEvent) {
    el.setPointerCapture(e.pointerId);
    drag = { x: e.clientX, vs, ve, box: (e.target as HTMLElement).classList.contains('box'), moved: false };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const dx = e.clientX - drag.x;
    if (Math.abs(dx) > 3) drag.moved = true;
    if (!drag.moved || !drag.box) return;
    const df = (dx / el.getBoundingClientRect().width) * (hi - lo);
    onView(drag.vs + df, drag.ve + df);
  }
  function up(e: PointerEvent) {
    const d = drag;
    drag = null;
    if (!d || d.moved) return;
    const hz = hzAt(e.clientX);
    const t = pickTick(ticks, hz, (hi - lo) / width);
    if (t) onTune(t.hz);
    else onGlide(hz - (ve - vs) / 2, hz + (ve - vs) / 2);
  }
</script>

<div
  class="strip"
  bind:this={el}
  bind:clientWidth={width}
  style:--hue={hue}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={() => (drag = null)}
  role="img"
  aria-label="The whole band. Click a station to tune it, anywhere else to look there; drag the box to move."
>
  {#each labels as f (f)}
    <span class="lab mono" style:left="{x(f)}%">{(f / 1e6).toFixed(f % 1e6 ? 1 : 0)}</span>
  {/each}
  {#each ticks as t (t.hz)}
    <span class="tick" class:fresh={t.fresh} class:saved={t.saved} style:left="{x(t.hz)}%" style:height="{5 + t.strength * 11}px" title={t.fresh ? t.title : `${t.title} — not heard since the app started`}></span>
  {/each}
  <span class="tuned" style:left="{x(tuned)}%"></span>
  <span class="box" style:left="calc({x((vs + ve) / 2)}% - {boxW / 2}px)" style:width="{boxW}px"></span>
</div>

<style>
  .strip {
    position: relative;
    flex: 0 0 auto;
    height: 30px;
    border-top: 1px solid var(--bg-light);
    background: var(--bg-dark);
    cursor: pointer;
    touch-action: none;
    overflow: hidden;
  }
  .lab {
    position: absolute;
    top: 2px;
    font-size: 10px;
    color: var(--fg-dim);
    transform: translateX(-50%);
    pointer-events: none;
  }
  .tick {
    position: absolute;
    bottom: 3px;
    width: 2px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--muted);
  }
  .tick.fresh {
    background: var(--fg);
  }
  .tick.saved {
    background: var(--yellow);
  }
  .tuned {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--hue);
    pointer-events: none;
  }
  .box {
    position: absolute;
    top: 1px;
    bottom: 1px;
    border: 1.5px solid var(--hue);
    border-radius: 3px;
    background: color-mix(in srgb, var(--hue) 14%, transparent);
    cursor: grab;
  }
</style>
