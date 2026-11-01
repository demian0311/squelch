<script lang="ts">
  import { channelTicks } from '../lib/channels';
  import { fmtMHz, outOfBand } from '../lib/radio';
  // Spectrum line on top, scrolling waterfall below. Rows arrive through push();
  // the visible frequency range (view) is owned by the parent.
  import { onMount } from 'svelte';
  import { lut as makeLut } from '../lib/theme';
  import { makeRenderer, type Renderer } from '../lib/waterfall/renderer';
  import type { Row } from '../lib/api';

  interface Props {
    vs: number;
    ve: number;
    rate: number;
    stops: string[];
    tuned: number;
    bandwidth: number;
    specH: number;
    colors: { bg: string; grid: string; line: string; fill: string; accent: string; tick: string };
    onTune: (hz: number, pxHz: number) => void;
    onView: (vs: number, ve: number) => void;
    /** The band's channel plan, shown on the scale instead of MHz when in view. */
    bandId?: string;
    /** The selected band's edges and name; the view past them is shaded. */
    band?: { lo: number; hi: number; name: string };
  }
  let { vs, ve, rate, stops, tuned, bandwidth, specH, colors, onTune, onView, bandId, band }: Props = $props();

  let wrap: HTMLDivElement;
  let specCanvas: HTMLCanvasElement;
  let wfCanvas: HTMLCanvasElement;
  let width = $state(800);
  let height = $state(500);
  let ctx: CanvasRenderingContext2D | null = null;
  let renderer: Renderer | null = null;
  // the spectrum line, one point per screen pixel, smoothed over rows
  let avg = new Float32Array(0);
  let lastVs = 0;
  let lastVe = 0;
  let dragging = $state(false);
  let drag: { x: number; vs: number; ve: number; moved: boolean } | null = null;
  // drawing happens in device pixels so the picture stays sharp on scaled displays
  let dpr = 1;
  let S = 0;
  let W = 0;
  /** device pixels per waterfall row and per spectrum point: whole screen pixels */
  let step = 1;

  const AXIS = 22;
  let wfH = $derived(Math.max(60, height - specH - AXIS));

  $effect(() => {
    renderer?.setPalette(makeLut(stops));
    renderer?.draw(vs, ve);
  });

  function resize() {
    if (!specCanvas || !wfCanvas) return;
    dpr = window.devicePixelRatio || 1;
    step = Math.max(1, Math.round(dpr));
    W = Math.max(64, Math.round(width * dpr));
    S = Math.round(specH * dpr);
    if (specCanvas.width !== W || specCanvas.height !== S) {
      specCanvas.width = W;
      specCanvas.height = S;
      ctx = specCanvas.getContext('2d');
    }
    renderer?.resize(W, Math.round(wfH * dpr), step);
    const points = Math.floor(W / step) + 1;
    if (avg.length !== points) {
      avg = new Float32Array(points);
      lastVs = vs;
      lastVe = ve;
      rangeKey = '';
    }
    drawSpectrum();
    renderer?.draw(vs, ve);
  }

  onMount(() => {
    renderer = makeRenderer(wfCanvas);
    renderer.setPalette(makeLut(stops));
    resize();
  });

  $effect(() => {
    void width;
    void wfH;
    void specH;
    resize();
  });

  // When the view moves or zooms, stretch and shift the line's history so it stays lined up; the waterfall does its own.
  $effect(() => {
    const nvs = vs;
    const nve = ve;
    if (nvs === lastVs && nve === lastVe) return;
    if (lastVe > lastVs) {
      const P = avg.length;
      const a = (lastVe - lastVs) / (nve - nvs);
      const b = ((lastVs - nvs) / (nve - nvs)) * P;
      const na = new Float32Array(P);
      for (let x = 0; x < P; x++) {
        const src = Math.round((x - b) / a);
        na[x] = src >= 0 && src < P ? avg[src] : 0;
      }
      avg = na;
    }
    lastVs = nvs;
    lastVe = nve;
    rangeKey = '';
    renderer?.draw(nvs, nve);
    drawSpectrum();
  });

  // which bins each spectrum point covers; recomputed only when the view, the capture or the width changes
  let b0s = new Int32Array(0);
  let b1s = new Int32Array(0);
  let rangeKey = '';
  function binRanges(lo: number, n: number) {
    const P = avg.length;
    const key = `${P} ${lo} ${n} ${rate} ${vs} ${ve}`;
    if (key === rangeKey) return;
    rangeKey = key;
    if (b0s.length !== P) {
      b0s = new Int32Array(P);
      b1s = new Int32Array(P);
    }
    const binHz = rate / n;
    const span = ve - vs;
    for (let i = 0; i < P; i++) {
      const b0 = Math.floor((vs + ((i - 0.5) / (P - 1)) * span - lo) / binHz);
      const b1 = Math.floor((vs + ((i + 0.5) / (P - 1)) * span - lo) / binHz);
      // b1 < 0 marks a point outside the capture
      if (b1 < 0 || b0 >= n) {
        b1s[i] = -1;
      } else {
        b0s[i] = Math.max(0, b0);
        b1s[i] = Math.min(n - 1, Math.max(b0, b1));
      }
    }
  }

  /** Add one spectrum row. */
  export function push(row: Row) {
    if (!renderer) return;
    const bins = row.bins;
    binRanges(row.centerHz - rate / 2, bins.length);
    for (let i = 0; i < avg.length; i++) {
      const b1 = b1s[i];
      let v = 0;
      if (b1 >= 0) for (let b = b0s[i]; b <= b1; b++) if (bins[b] > v) v = bins[b];
      avg[i] = avg[i] * 0.6 + v * 0.4;
    }
    renderer.push(bins, row.centerHz, rate, vs, ve);
    renderer.draw(vs, ve);
    drawSpectrum();
  }

  function drawSpectrum() {
    if (!ctx) return;
    ctx.fillStyle = colors.bg;
    ctx.fillRect(0, 0, W, S);
    ctx.strokeStyle = colors.grid;
    ctx.lineWidth = dpr;
    for (let g = 1; g < 4; g++) {
      const y = Math.round((S * g) / 4) + 0.5;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(W, y);
      ctx.stroke();
    }
    // tuned channel band
    const span = ve - vs;
    const x0 = ((tuned - bandwidth / 2 - vs) / span) * W;
    const x1 = ((tuned + bandwidth / 2 - vs) / span) * W;
    ctx.fillStyle = colors.fill;
    ctx.fillRect(x0, 0, Math.max(2 * dpr, x1 - x0), S);
    const P = avg.length;
    if (P < 2) return;
    const y = (i: number) => S - (avg[i] / 255) * (S - 10 * dpr) - dpr;
    // one path for both the fill and the line
    const curve = new Path2D();
    curve.moveTo(0, y(0));
    for (let i = 1; i < P; i++) curve.lineTo((i / (P - 1)) * W, y(i));
    const under = new Path2D(curve);
    under.lineTo(W, S);
    under.lineTo(0, S);
    under.closePath();
    ctx.fillStyle = colors.fill;
    ctx.fill(under);
    ctx.strokeStyle = colors.line;
    ctx.stroke(curve);
  }

  $effect(() => {
    void tuned;
    void bandwidth;
    void colors;
    drawSpectrum();
  });

  function freqAt(clientX: number) {
    const r = wrap.getBoundingClientRect();
    return vs + Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * (ve - vs);
  }

  function down(e: PointerEvent) {
    wrap.setPointerCapture(e.pointerId);
    drag = { x: e.clientX, vs, ve, moved: false };
    dragging = true;
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const dx = e.clientX - drag.x;
    if (Math.abs(dx) > 4) drag.moved = true;
    if (!drag.moved) return;
    const r = wrap.getBoundingClientRect();
    const df = (-dx / r.width) * (drag.ve - drag.vs);
    onView(drag.vs + df, drag.ve + df);
  }
  function up(e: PointerEvent) {
    const d = drag;
    drag = null;
    dragging = false;
    if (d && !d.moved) onTune(freqAt(e.clientX), (ve - vs) / wrap.getBoundingClientRect().width);
  }
  /** Zoom per pixel scrolled: 100 px is about 20 %. Touchpads send many small events, wheels a few big ones. */
  const ZOOM_PER_PX = 0.002;
  const LINE_PX = 16;

  // vertical scroll zooms around the pointer; sideways scroll moves the view
  function wheel(e: WheelEvent) {
    e.preventDefault();
    const unit = e.deltaMode === 1 ? LINE_PX : e.deltaMode === 2 ? wrap.clientHeight : 1;
    const dx = e.deltaX * unit;
    const dy = e.deltaY * unit;
    const span = ve - vs;
    if (Math.abs(dx) > Math.abs(dy)) {
      const shift = (dx / wrap.getBoundingClientRect().width) * span;
      onView(vs + shift, ve + shift);
      return;
    }
    const f = freqAt(e.clientX);
    const k = Math.exp(dy * ZOOM_PER_PX);
    onView(f - (f - vs) * k, f + (ve - f) * k);
  }

  function niceStep(span: number) {
    const raw = span / 7;
    const p = Math.pow(10, Math.floor(Math.log10(raw)));
    const m = raw / p;
    return (m < 1.5 ? 1 : m < 3.5 ? 2 : m < 7.5 ? 5 : 10) * p;
  }
  let ticks = $derived.by(() => {
    const span = ve - vs;
    const step = niceStep(span);
    const dec = Math.max(0, Math.min(4, 6 - Math.floor(Math.log10(step))));
    const out: { left: number; text: string }[] = [];
    for (let f = Math.ceil(vs / step) * step; f < ve; f += step) {
      const left = ((f - vs) / span) * 100;
      if (left > 2 && left < 98) out.push({ left, text: (f / 1e6).toFixed(dec) });
    }
    return out;
  });
  let chTicks = $derived(channelTicks(bandId, vs, ve, width));
  let shade = $derived(band ? outOfBand(vs, ve, band.lo, band.hi) : { below: 0, above: 0 });
  let markerLeft = $derived(((tuned - vs) / (ve - vs)) * 100);
</script>

<div
  class="wrap"
  class:dragging
  bind:this={wrap}
  bind:clientWidth={width}
  bind:clientHeight={height}
  style:background={colors.bg}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={() => ((drag = null), (dragging = false))}
  onwheel={wheel}
  role="img"
  aria-label="Spectrum and waterfall. Drag to move, scroll to zoom, click to tune."
>
  <canvas bind:this={specCanvas} class="spec" style:height="{specH}px"></canvas>
  <canvas bind:this={wfCanvas} class="wf" style:top="{specH}px" style:height="{wfH}px"></canvas>
  {#if band && shade.below > 0}
    <div class="out below" style:width="{shade.below}%" style:height="{specH + wfH}px">
      {#if shade.below < 100}<span class="mono">{band.name} starts {fmtMHz(band.lo)}</span>{/if}
    </div>
  {/if}
  {#if band && shade.above > 0}
    <div class="out above" style:width="{shade.above}%" style:height="{specH + wfH}px">
      {#if shade.above < 100}<span class="mono">{band.name} ends {fmtMHz(band.hi)}</span>{/if}
    </div>
  {/if}
  {#if markerLeft >= 0 && markerLeft <= 100}
    <div class="marker" style:left="{markerLeft}%" style:height="{specH + wfH}px" style:background={colors.accent}></div>
  {/if}
  <div class="axis" style:top="{specH + wfH}px">
    {#if chTicks.length}
      {#each chTicks as t (t.left)}
        <i class="mark" style:left="{t.left}%" style:background={colors.tick}></i>
        {#if t.text}<span class="mono ch" style:left="{t.left}%" style:color={colors.tick}>{t.text}</span>{/if}
      {/each}
    {:else}
      {#each ticks as t (t.text)}
        <span class="mono" style:left="{t.left}%" style:color={colors.tick}>{t.text}</span>
      {/each}
    {/if}
  </div>
</div>

<style>
  .wrap {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    cursor: crosshair;
    touch-action: none;
  }
  .wrap.dragging {
    cursor: grabbing;
  }
  canvas {
    display: block;
    width: 100%;
    pointer-events: none;
  }
  .wf {
    position: absolute;
    left: 0;
  }
  /* outside the selected band: dimmed, with a dashed line at the edge */
  .out {
    position: absolute;
    top: 0;
    background: color-mix(in srgb, var(--bg-dark, #000) 55%, transparent);
    pointer-events: none;
  }
  .out.below {
    left: 0;
    border-right: 1px dashed var(--yellow);
  }
  .out.above {
    right: 0;
    border-left: 1px dashed var(--yellow);
  }
  .out span {
    position: absolute;
    top: 4px;
    color: var(--yellow);
    font-size: 11px;
    white-space: nowrap;
  }
  .out.below span {
    right: 6px;
  }
  .out.above span {
    left: 6px;
  }
  .marker {
    position: absolute;
    top: 0;
    width: 2px;
    margin-left: -1px;
    opacity: 0.9;
    pointer-events: none;
  }
  .axis {
    position: absolute;
    left: 0;
    right: 0;
    height: 22px;
    pointer-events: none;
  }
  .axis span {
    position: absolute;
    top: 4px;
    transform: translateX(-50%);
    font-size: 11px;
    white-space: nowrap;
  }
  .axis .ch {
    top: 6px;
  }
  .axis .mark {
    position: absolute;
    top: 0;
    width: 1px;
    height: 4px;
    opacity: 0.6;
  }
</style>
