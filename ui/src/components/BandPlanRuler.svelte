<script lang="ts">
  // What each part of the band is for, as a ruler above the waterfall (data in lib/bandplan.ts).
  // Hover a block to read it, click it to tune its middle in its mode. The chevron folds the ruler
  // to a thin stripe of the same colours; a click on the stripe, or `p`, brings it back.
  import { USE_WORDS, segmentTarget, segmentsIn } from '../lib/bandplan';
  import { fmtMHz } from '../lib/radio';
  import Icon from './Icon.svelte';

  let {
    bandId,
    vs,
    ve,
    open,
    onToggle,
    onTune,
  }: {
    bandId: string;
    vs: number;
    ve: number;
    open: boolean;
    onToggle: () => void;
    onTune: (hz: number) => void;
  } = $props();

  /** About one character of the 10 px mono face, with the block's padding. */
  const CHAR_PX = 6.2;
  const PAD_PX = 8;

  let width = $state(1000);
  let segs = $derived(segmentsIn(bandId, vs, ve));
  /** The name if it fits the block, else its part before " · ", else nothing. */
  function fitName(name: string, px: number): string {
    for (const n of [name, name.split(' · ')[0]]) if (px >= n.length * CHAR_PX + PAD_PX) return n;
    return '';
  }
  const tip = (s: (typeof segs)[number]) => `${s.name} · ${fmtMHz(s.lo)}–${fmtMHz(s.hi)} MHz\n${USE_WORDS[s.use]}`;
</script>

{#if open}
  <div class="plan" bind:clientWidth={width}>
    {#each segs as s (s.lo)}
      <button class="seg {s.use}" style:left="{s.left}%" style:width="{s.width}%" title={tip(s)} onclick={() => onTune(segmentTarget(s, vs, ve))}>
        <span class="mono">{fitName(s.name, (s.width / 100) * width)}</span>
      </button>
    {/each}
    <button class="fold" title="Hide the band plan (p)" aria-label="Hide the band plan" onclick={onToggle}><Icon name="chevronUp" size={12} /></button>
  </div>
{:else}
  <button class="plan thin" title="Show what each part of the band is for (p)" aria-label="Show the band plan" onclick={onToggle}>
    {#each segs as s (s.lo)}
      <span class="seg {s.use}" style:left="{s.left}%" style:width="{s.width}%"></span>
    {/each}
    <span class="fold"><Icon name="chevronDown" size={10} /></span>
  </button>
{/if}

<style>
  .plan {
    position: relative;
    flex: 0 0 auto;
    height: 18px;
    background: var(--bg-dark);
    border-bottom: 1px solid var(--bg-light);
    overflow: hidden;
  }
  .plan.thin {
    display: block;
    width: 100%;
    height: 10px;
    padding: 0;
    border: 0;
    border-bottom: 1px solid var(--bg-light);
    border-radius: 0;
    cursor: pointer;
  }
  .seg {
    position: absolute;
    top: 2px;
    bottom: 2px;
    padding: 0;
    border: 0;
    border-right: 1px solid var(--bg-dark);
    border-radius: 2px;
    background: color-mix(in srgb, var(--c) 62%, var(--bg-dark));
    color: var(--bg-darker);
    font-size: 10px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    cursor: pointer;
  }
  .thin .seg {
    top: 2px;
    bottom: 2px;
  }
  .thin .fold {
    width: 18px;
  }
  .thin:hover .seg {
    background: var(--c);
  }
  .thin:hover .fold {
    color: var(--fg);
  }
  button.seg:hover,
  button.seg:focus-visible {
    background: var(--c);
  }
  .weak { --c: var(--accent); }
  .data { --c: var(--cyan); }
  .beacon { --c: var(--fg-dim); }
  .sat { --c: var(--orange); }
  .rout, .rpt { --c: var(--green); }
  .rin {
    --c: var(--green);
    background-image: repeating-linear-gradient(135deg, color-mix(in srgb, var(--bg-darker) 45%, transparent) 0 3px, transparent 3px 6px);
  }
  .simplex { --c: var(--yellow); }
  .other {
    --c: var(--muted);
    color: var(--fg-dim);
  }
  .fold {
    position: absolute;
    right: 0;
    top: 0;
    bottom: 0;
    width: 18px;
    padding: 0;
    display: grid;
    place-items: center;
    border: 0;
    border-left: 1px solid var(--bg-light);
    border-radius: 0;
    background: var(--bg-dark);
    color: var(--fg-dim);
    cursor: pointer;
  }
  .fold:hover {
    color: var(--fg);
  }
</style>
