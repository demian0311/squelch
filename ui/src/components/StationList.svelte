<script lang="ts">
  // The dial as a table, sorted by a column head. A blue edge marks stations in the waterfall's view; a grey bar is the last reading.
  import Level from './Level.svelte';
  import { fmChannel } from '../lib/radio';
  import { firstDesc, holdSignals, sortDial, type DialRow, type DialSort } from '../lib/stations';
  import * as store from '../lib/store';

  let { rows, hue, tuned, vs, ve, meterDb, onTune }: { rows: DialRow[]; hue: string; tuned: number; vs: number; ve: number; meterDb: number; onTune: (hz: number) => void } = $props();

  const saved = store.getState<{ key: DialSort; desc: boolean }>('stations.sort', { key: 'freq', desc: false });
  let sortKey = $state(saved.key);
  let desc = $state(saved.desc);
  // held readings keep a row in place while its live one wanders; cleared whenever the order is not by signal
  let held = new Map<number, number | null>();
  let sorted = $derived.by(() => {
    held = sortKey === 'signal' ? holdSignals(held, rows) : new Map();
    return sortDial(rows, sortKey, desc, held);
  });
  function sortBy(key: DialSort) {
    desc = key === sortKey ? !desc : firstDesc(key);
    sortKey = key;
    store.setState('stations.sort', { key, desc });
  }

  let list = $state<HTMLElement>();
  let head = $state<HTMLElement>();
  let placed = false;
  let channel = $derived(fmChannel(tuned));
  // only when the tuned channel or the order's column changes, or rows first arrive, so a status never costs a layout read
  let hasRows = $derived(rows.length > 0);
  $effect(() => {
    void sortKey;
    void desc;
    if (!hasRows || !list) return;
    const row = list.querySelector<HTMLElement>(`[data-hz="${channel}"]`);
    if (!row) return;
    const under = head?.offsetHeight ?? 0;
    const top = row.offsetTop;
    if (!placed || top < list.scrollTop + under || top + row.offsetHeight > list.scrollTop + list.clientHeight) list.scrollTop = top - list.clientHeight / 2;
    placed = true;
  });
</script>

{#snippet sortHead(key: DialSort, label: string, cls: string)}
  <button class="hc {cls}" class:on={sortKey === key} aria-pressed={sortKey === key} onclick={() => sortBy(key)} title="Sort by {label.toLowerCase()}">
    {label}<i>{sortKey === key ? (desc ? '▼' : '▲') : ''}</i>
  </button>
{/snippet}

<div class="stations" style:--c={hue} bind:this={list}>
  <div class="head" bind:this={head}>
    {@render sortHead('freq', 'MHz', 'f')}
    {@render sortHead('call', 'Call sign', 'n')}
    {@render sortHead('city', 'City', 'c')}
    <span class="hc t">Type</span>
    <span class="hc h">HD</span>
    {@render sortHead('signal', 'Signal', 'l')}
  </div>
  {#each sorted as r (r.hz)}
    <button class="row" class:view={r.hz >= vs && r.hz <= ve} class:tuned={r.hz === channel} data-hz={r.hz} onclick={() => onTune(r.hz)} title="Tune to {r.title}">
      <span class="f">{(r.hz / 1e6).toFixed(1)}</span>
      <span class="n">{r.title}</span>
      <span class="c">{r.city ?? ''}</span>
      <span class="t">{r.type ?? ''}</span>
      <span class="h">{#if r.hd}<b title="Also broadcasts digitally (HD Radio)">HD</b>{/if}</span>
      <span class="l" class:old={!r.onAir}>{#if r.snrDb !== null}<Level pct={(r.snrDb / meterDb) * 100} width="100%" height="5px" />{/if}</span>
    </button>
  {/each}
</div>

<style>
  .stations {
    --cols: 44px 72px minmax(0, 1fr) 92px 24px 56px;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    position: relative;
    container-type: inline-size;
    padding-bottom: 4px;
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: var(--cols);
    gap: 10px;
    align-items: center;
    padding: 3px 12px 3px 10px;
    border-left: 2px solid transparent;
  }
  .head {
    position: sticky;
    top: 0;
    z-index: 1;
    padding-block: 5px;
    background: var(--bg-dark);
    border-bottom: 1px solid var(--muted);
  }
  .hc {
    display: block;
    min-width: 0;
    padding: 0;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--fg-dim);
    font: inherit;
    font-size: 11.5px;
    text-align: left;
    white-space: nowrap;
  }
  button.hc {
    cursor: pointer;
  }
  button.hc:hover,
  button.hc:focus-visible,
  .hc.on {
    color: var(--fg);
    outline: none;
  }
  .hc i {
    margin-left: 3px;
    font-style: normal;
    font-size: 8px;
  }
  .row {
    width: 100%;
    border-block: 0;
    border-right: 0;
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
    text-align: right;
  }
  .row .f {
    font-family: var(--mono);
    font-size: 12.5px;
    color: var(--fg-dim);
  }
  .view .f,
  .tuned .f {
    color: inherit;
  }
  .row .n {
    font-weight: 500;
  }
  .n,
  .c,
  .t {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row .c,
  .row .t {
    font-size: 12px;
    color: var(--fg-dim);
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
  /* Type goes first: the three columns that sort stay */
  @container (max-width: 400px) {
    .head,
    .row {
      grid-template-columns: 44px 62px minmax(0, 1fr) 24px 44px;
    }
    .t {
      display: none;
    }
  }
</style>
