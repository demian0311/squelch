<script lang="ts">
  // The dial as a list. A blue edge marks stations in the waterfall's view; a grey bar is the last reading.
  import Level from './Level.svelte';
  import { fmChannel } from '../lib/radio';
  import type { DialRow } from '../lib/stations';

  let { rows, hue, tuned, vs, ve, meterDb, onTune }: { rows: DialRow[]; hue: string; tuned: number; vs: number; ve: number; meterDb: number; onTune: (hz: number) => void } = $props();

  let list = $state<HTMLElement>();
  let placed = false;
  // only when the tuned channel changes, so a status never costs a layout read; the first time puts it mid-list
  let channel = $derived(fmChannel(tuned));
  $effect(() => {
    const row = list?.querySelector<HTMLElement>(`[data-hz="${channel}"]`);
    if (!list || !row) return;
    const top = row.offsetTop - list.offsetTop;
    if (!placed || top < list.scrollTop || top + row.offsetHeight > list.scrollTop + list.clientHeight) list.scrollTop = top - list.clientHeight / 2;
    placed = true;
  });
</script>

<div class="stations" style:--c={hue} bind:this={list}>
  {#each rows as r (r.hz)}
    <button class="row" class:view={r.hz >= vs && r.hz <= ve} class:tuned={r.hz === channel} data-hz={r.hz} onclick={() => onTune(r.hz)} title="Tune to {r.title}">
      <span class="f">{(r.hz / 1e6).toFixed(1)}</span>
      <span class="n">{r.title}{#if r.city}<em>{r.city}</em>{/if}</span>
      <span class="t">{r.type ?? ''}</span>
      <span class="h">{#if r.hd}<b title="Also broadcasts digitally (HD Radio)">HD</b>{/if}</span>
      <span class="l" class:old={!r.onAir}>{#if r.snrDb !== null}<Level pct={(r.snrDb / meterDb) * 100} width="100%" height="5px" />{/if}</span>
    </button>
  {/each}
</div>

<style>
  .stations {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    position: relative;
    container-type: inline-size;
    padding-block: 4px;
  }
  .row {
    display: grid;
    grid-template-columns: 44px minmax(0, 1fr) 92px 24px 44px;
    gap: 10px;
    align-items: center;
    width: 100%;
    padding: 3px 12px 3px 10px;
    border: 0;
    border-left: 2px solid transparent;
    border-radius: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .row:hover,
  .row:focus-visible {
    background: var(--bg-light);
    outline: none;
  }
  .row.view {
    border-left-color: var(--c);
  }
  .row.tuned {
    background: color-mix(in srgb, var(--c) 28%, transparent);
  }
  .f {
    font-family: var(--mono);
    font-size: 12.5px;
    text-align: right;
    color: var(--fg-dim);
  }
  .view .f,
  .tuned .f {
    color: inherit;
  }
  .n {
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .n em {
    margin-left: 6px;
    font-style: normal;
    font-weight: 400;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .t {
    font-size: 12px;
    color: var(--fg-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .h b {
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 500;
    padding: 0 3px;
    border: 1px solid var(--orange);
    border-radius: 3px;
    color: var(--orange);
  }
  .l {
    display: block;
  }
  .l.old {
    filter: grayscale(1);
    opacity: 0.6;
  }
  @container (max-width: 340px) {
    .row {
      grid-template-columns: 44px minmax(0, 1fr) 24px 36px;
    }
    .t {
      display: none;
    }
  }
</style>
