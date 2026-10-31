<script lang="ts">
  // Spectrum line on top, scrolling waterfall below. Rows arrive through push();
  // the visible frequency range (view) is owned by the parent.
  import { onMount } from 'svelte';
  import { lut as makeLut } from '../lib/theme';
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
  }
  let { vs, ve, rate, stops, tuned, bandwidth, specH, colors, onTune, onView }: Props = $props();

  let canvas: HTMLCanvasElement;
  let width = $state(800);
  let height = $state(500);
  let ctx: CanvasRenderingContext2D | null = null;
  let img: ImageData | null = null;
  let avg = new Float32Array(0);
  let lastVs = 0;
  let lastVe = 0;
  let lutCache: Uint8ClampedArray = new Uint8ClampedArray(256 * 3);
  let dragging = $state(false);
  let drag: { x: number; vs: number; ve: number; moved: boolean } | null = null;
  let spectrumDirty = false;

  const AXIS = 22;
  let wfH = $derived(Math.max(60, height - specH - AXIS));

  $effect(() => {
    lutCache = makeLut(stops);
  });

  function resize() {
    if (!canvas) return;
    const w = Math.max(64, Math.floor(width));
    const h = specH + wfH;
    if (canvas.width === w && canvas.height === h) return;
    const old = ctx && canvas.width > 0 ? ctx.getImageData(0, specH, canvas.width, Math.min(canvas.height - specH, wfH)) : null;
    canvas.width = w;
    canvas.height = h;
    ctx = canvas.getContext('2d', { willReadFrequently: false });
    if (!ctx) return;
    ctx.fillStyle = stops[0];
    ctx.fillRect(0, 0, w, h);
    if (old) ctx.putImageData(old, 0, specH);
    img = ctx.createImageData(w, 1);
    avg = new Float32Array(w);
    lastVs = vs;
    lastVe = ve;
  }

  $effect(() => {
    void width;
    void wfH;
    resize();
  });

  // When the view moves or zooms, stretch and shift the history so it stays lined up.
  $effect(() => {
    const nvs = vs;
    const nve = ve;
    if (!ctx || (nvs === lastVs && nve === lastVe)) return;
    const W = canvas.width;
    const a = (lastVe - lastVs) / (nve - nvs);
    const b = ((lastVs - nvs) / (nve - nvs)) * W;
    const tmp = document.createElement('canvas');
    tmp.width = W;
    tmp.height = wfH;
    tmp.getContext('2d')!.drawImage(canvas, 0, specH, W, wfH, 0, 0, W, wfH);
    ctx.fillStyle = stops[0];
    ctx.fillRect(0, specH, W, wfH);
    ctx.drawImage(tmp, 0, 0, W, wfH, b, specH, a * W, wfH);
    const na = new Float32Array(W);
    for (let x = 0; x < W; x++) {
      const src = Math.round((x - b) / a);
      na[x] = src >= 0 && src < W ? avg[src] : 0;
    }
    avg = na;
    lastVs = nvs;
    lastVe = nve;
    spectrumDirty = true;
  });

  /** Add one spectrum row, mapped from its capture window into the current view. */
  export function push(row: Row) {
    if (!ctx || !img) return;
    const W = canvas.width;
    const n = row.bins.length;
    const lo = row.centerHz - rate / 2;
    const binHz = rate / n;
    const span = ve - vs;
    const data = img.data;
    const L = lutCache;
    for (let x = 0; x < W; x++) {
      const f0 = vs + (x / W) * span;
      const f1 = vs + ((x + 1) / W) * span;
      let b0 = Math.floor((f0 - lo) / binHz);
      let b1 = Math.floor((f1 - lo) / binHz);
      let v = 0;
      if (b1 < 0 || b0 >= n) {
        v = 0;
      } else {
        b0 = Math.max(0, b0);
        b1 = Math.min(n - 1, Math.max(b0, b1));
        for (let b = b0; b <= b1; b++) if (row.bins[b] > v) v = row.bins[b];
      }
      avg[x] = avg[x] * 0.6 + v * 0.4;
      const k = v * 3;
      data[x * 4] = L[k];
      data[x * 4 + 1] = L[k + 1];
      data[x * 4 + 2] = L[k + 2];
      data[x * 4 + 3] = 255;
    }
    ctx.drawImage(canvas, 0, specH, W, wfH - 1, 0, specH + 1, W, wfH - 1);
    ctx.putImageData(img, 0, specH);
    spectrumDirty = true;
  }

  function drawSpectrum() {
    if (!ctx || !spectrumDirty) return;
    spectrumDirty = false;
    const W = canvas.width;
    ctx.fillStyle = colors.bg;
    ctx.fillRect(0, 0, W, specH);
    ctx.strokeStyle = colors.grid;
    ctx.lineWidth = 1;
    for (let g = 1; g < 4; g++) {
      const y = Math.round((specH * g) / 4) + 0.5;
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
    ctx.fillRect(x0, 0, Math.max(2, x1 - x0), specH);
    const y = (x: number) => specH - (avg[x] / 255) * (specH - 10) - 1;
    ctx.beginPath();
    ctx.moveTo(0, specH);
    for (let x = 0; x < W; x++) ctx.lineTo(x, y(x));
    ctx.lineTo(W, specH);
    ctx.closePath();
    ctx.fillStyle = colors.fill;
    ctx.fill();
    ctx.beginPath();
    for (let x = 0; x < W; x++) (x === 0 ? ctx.moveTo : ctx.lineTo).call(ctx, x, y(x));
    ctx.strokeStyle = colors.line;
    ctx.stroke();
  }

  onMount(() => {
    let raf = 0;
    const loop = () => {
      drawSpectrum();
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  $effect(() => {
    void tuned;
    void bandwidth;
    void colors;
    spectrumDirty = true;
  });

  function freqAt(clientX: number) {
    const r = canvas.getBoundingClientRect();
    return vs + Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * (ve - vs);
  }

  function down(e: PointerEvent) {
    canvas.setPointerCapture(e.pointerId);
    drag = { x: e.clientX, vs, ve, moved: false };
    dragging = true;
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const dx = e.clientX - drag.x;
    if (Math.abs(dx) > 4) drag.moved = true;
    if (!drag.moved) return;
    const r = canvas.getBoundingClientRect();
    const df = (-dx / r.width) * (drag.ve - drag.vs);
    onView(drag.vs + df, drag.ve + df);
  }
  function up(e: PointerEvent) {
    const d = drag;
    drag = null;
    dragging = false;
    if (d && !d.moved) onTune(freqAt(e.clientX), (ve - vs) / canvas.width);
  }
  function wheel(e: WheelEvent) {
    e.preventDefault();
    const f = freqAt(e.clientX);
    const k = e.deltaY > 0 ? 1.25 : 0.8;
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
  let markerLeft = $derived(((tuned - vs) / (ve - vs)) * 100);
</script>

<div class="wrap" bind:clientWidth={width} bind:clientHeight={height} style:background={colors.bg}>
  <canvas
    bind:this={canvas}
    class:dragging
    style:height="{specH + wfH}px"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={() => ((drag = null), (dragging = false))}
    onwheel={wheel}
    aria-label="Spectrum and waterfall. Drag to move, scroll to zoom, click to tune."
  ></canvas>
  {#if markerLeft >= 0 && markerLeft <= 100}
    <div class="marker" style:left="{markerLeft}%" style:height="{specH + wfH}px" style:background={colors.accent}></div>
  {/if}
  <div class="axis" style:top="{specH + wfH}px">
    {#each ticks as t (t.text)}
      <span class="mono" style:left="{t.left}%" style:color={colors.tick}>{t.text}</span>
    {/each}
  </div>
</div>

<style>
  .wrap {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }
  canvas {
    display: block;
    width: 100%;
    cursor: crosshair;
    touch-action: none;
  }
  canvas.dragging {
    cursor: grabbing;
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
</style>
