<script lang="ts">
  // All of EMS in one picture above the live waterfall: one row per whole-band check, newest on top, with the empty
  // stretch between the two bands closed up. Click a line to tune it. The box is what the waterfall below shows.
  import { fmtMHz } from '../lib/radio';
  import { cellsToPixels, stripHz, stripPick, stripSay, stripSections, stripX, STRIP_ROWS, type LookLine } from '../lib/ems';

  let {
    ranges,
    lines,
    looking,
    agoS,
    vs,
    ve,
    tuned,
    scan = null,
    tried = [],
    palette,
    hue,
    onTune,
  }: {
    ranges: [number, number][];
    lines: LookLine[];
    looking: boolean;
    agoS: number | null;
    vs: number;
    ve: number;
    tuned: number;
    /** The hop a control-channel scan is on, outlined. */
    scan?: { lo_hz: number; hi_hz: number } | null;
    /** Carriers the scan has tested, ticked under the picture. */
    tried?: number[];
    palette: Uint8ClampedArray;
    hue: string;
    onTune: (hz: number) => void;
  } = $props();

  /** Pixels per check, so the picture's height never changes. */
  const ROW_H = 6;
  /** MHz between labels on the scale, by band width. */
  const labelStep = (mhz: number) => (mhz > 10 ? 3 : 2);

  let width = $state(0);
  let hover = $state<number | null>(null);
  let canvas = $state<HTMLCanvasElement>();
  let sections = $derived(stripSections(ranges, width));
  let hzPerPx = $derived(sections[0] ? (sections[0].hi - sections[0].lo) / sections[0].w : 1);
  let labels = $derived(
    sections.flatMap((s) => {
      const step = labelStep((s.hi - s.lo) / 1e6);
      const out: { x: number; text: string }[] = [];
      for (let m = Math.ceil(s.lo / 1e6 / step) * step; m * 1e6 < s.hi; m += step) out.push({ x: s.x + ((m * 1e6 - s.lo) / (s.hi - s.lo)) * s.w, text: String(m) });
      return out;
    }),
  );

  function draw() {
    if (!canvas || width < 1) return;
    canvas.width = width;
    canvas.height = STRIP_ROWS * ROW_H;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const img = ctx.createImageData(width, canvas.height);
    const put = (x: number, y: number, v: number) => {
      for (let r = 0; r < ROW_H; r++) {
        const o = ((y * ROW_H + r) * width + x) * 4;
        img.data[o] = palette[v * 3];
        img.data[o + 1] = palette[v * 3 + 1];
        img.data[o + 2] = palette[v * 3 + 2];
        img.data[o + 3] = 255;
      }
    };
    lines.forEach((line, y) => {
      sections.forEach((s, i) => {
        const part = line.parts[i];
        if (!part) return;
        const x0 = Math.round(s.x);
        const pixels = Math.round(s.x + s.w) - x0;
        cellsToPixels(part.cells, pixels).forEach((v, x) => put(x0 + x, y, v));
      });
    });
    ctx.putImageData(img, 0, 0);
  }
  $effect(() => {
    void lines;
    void sections;
    void palette;
    draw();
  });

  const at = (e: MouseEvent) => e.clientX - (e.currentTarget as HTMLElement).getBoundingClientRect().left;
  function click(e: MouseEvent) {
    const hz = stripHz(sections, at(e));
    if (hz !== null) onTune(stripPick(lines, hz, hzPerPx));
  }
  let viewA = $derived(stripX(sections, Math.max(vs, ranges.find(([lo, hi]) => (vs + ve) / 2 >= lo && (vs + ve) / 2 <= hi)?.[0] ?? vs)));
  let viewB = $derived(stripX(sections, Math.min(ve, ranges.find(([lo, hi]) => (vs + ve) / 2 >= lo && (vs + ve) / 2 <= hi)?.[1] ?? ve)));
  let tunedX = $derived(stripX(sections, tuned));
  let hopA = $derived(scan ? stripX(sections, scan.lo_hz) : null);
  let hopB = $derived(scan ? stripX(sections, scan.hi_hz) : null);
  let triedX = $derived(tried.map((hz) => stripX(sections, hz)).filter((x): x is number => x !== null));
  let say = $derived(stripSay(looking, lines.length, agoS));
</script>

<div class="strip" style:--hue={hue} bind:clientWidth={width}>
  <div class="scale mono">
    {#each labels as l (l.text)}<span style:left="{l.x}px">{l.text}</span>{/each}
  </div>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="pic"
    style:height="{STRIP_ROWS * ROW_H}px"
    role="button"
    tabindex="-1"
    aria-label="All of EMS. Click a line to tune it."
    title={hover === null ? '' : `${fmtMHz(hover)} MHz`}
    onclick={click}
    onmousemove={(e) => (hover = stripHz(sections, at(e)))}
    onmouseleave={() => (hover = null)}
  >
    <canvas bind:this={canvas}></canvas>
    {#each sections.slice(1) as s}<span class="gap" style:left="{s.x - 10}px"></span>{/each}
    {#if viewA !== null && viewB !== null}<span class="view" style:left="{viewA}px" style:width="{Math.max(viewB - viewA, 4)}px"></span>{/if}
    {#if tunedX !== null}<span class="tuned" style:left="{tunedX}px"></span>{/if}
    {#if hopA !== null && hopB !== null}<span class="hop" style:left="{hopA}px" style:width="{Math.max(hopB - hopA, 4)}px"></span>{/if}
    {#each triedX as x (x)}<span class="tried" style:left="{x}px"></span>{/each}
  </div>
  <div class="say">{say}</div>
</div>

<style>
  .strip {
    flex: 0 0 auto;
    min-width: 0;
    background: var(--bg-dark);
    border-bottom: 1px solid var(--bg-light);
  }
  .scale {
    position: relative;
    height: 15px;
    font-size: 10px;
    color: var(--fg-dim);
    overflow: hidden;
  }
  .scale span {
    position: absolute;
    top: 1px;
    transform: translateX(-50%);
  }
  .pic {
    position: relative;
    cursor: pointer;
    overflow: hidden;
    background: var(--bg-dark);
  }
  /* out of the layout: its pixel width must not hold the pane open when a panel narrows it */
  canvas {
    position: absolute;
    inset: 0;
  }
  .gap {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 10px;
    background: var(--bg-light);
    pointer-events: none;
  }
  .view {
    position: absolute;
    top: 0;
    bottom: 0;
    border: 1px solid var(--hue);
    background: color-mix(in srgb, var(--hue) 14%, transparent);
    pointer-events: none;
  }
  .tuned {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--hue);
    pointer-events: none;
  }
  .hop {
    position: absolute;
    top: 0;
    bottom: 0;
    border: 1px dashed var(--hue);
    pointer-events: none;
  }
  .tried {
    position: absolute;
    bottom: 0;
    height: 40%;
    width: 1px;
    background: var(--fg-dim);
    opacity: 0.7;
    pointer-events: none;
  }
  .say {
    font-size: 11px;
    line-height: 16px;
    padding-inline: 6px;
    color: var(--fg-dim);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
</style>
