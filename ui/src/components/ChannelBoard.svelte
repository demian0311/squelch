<script lang="ts">
  // A band's channels as tiles in strips by what they are for, a word on those with a special use and a bar on each while
  // it is on air. A channel with a twin frequency is two halves (ship, shore), or a link in the detail line (repeater input).
  // A short panel drops the legend, then the strip headings, then shrinks the tiles, then drops the detail line.
  import { channelByKey, channelOn, sideOf, type Board, type BoardChannel } from '../lib/board';
  import { fmtMHz } from '../lib/radio';

  interface Props {
    board: Board;
    /** Strongest signal per channel key, dB above the floor. */
    live: Map<string, number>;
    tuned: number;
    /** Sides in the capture now; channels on another side are not heard. */
    sides: number[];
    /** dB that fills a bar. */
    fullDb: number;
    onTune: (hz: number) => void;
  }
  let { board, live, tuned, sides, fullDb, onTune }: Props = $props();

  let here = $derived(channelOn(board, tuned));
  let twin = $derived(here?.twin ? channelByKey(board, here.twin) : undefined);
  let hasDot = $derived(board.channels.some((c) => c.dot));

  const heard = (hz: number) => sides.includes(sideOf(board, hz));
  const on = (c: BoardChannel) => live.has(c.key) && heard(c.hz);
  const pct = (c: BoardChannel) => Math.max(8, Math.min(100, ((live.get(c.key) ?? 0) / fullDb) * 100));
  const title = (c: BoardChannel) => `${fmtMHz(c.hz, 4)} MHz${on(c) ? ' · on air' : heard(c.hz) ? '' : ' · other side, not heard'}. ${c.use}`;
  let anyAway = $derived(board.sides.some((_, i) => !sides.includes(i)));
</script>

{#snippet bar(c: BoardChannel)}
  <span class="bar"><i style:transform="scaleX({on(c) ? pct(c) / 100 : 0})"></i></span>
{/snippet}

<div class="board">
  <div class="inner">
    {#each board.strips as s (s.name)}
      <div class="strip">
        <div class="lbl"><span><b>{s.name}</b></span>{#if s.note}<span>{s.note}</span>{/if}</div>
        <div class="tiles">
          {#each s.keys as key (key)}
            {@const c = channelByKey(board, key)!}
            {@const other = c.twin ? channelByKey(board, c.twin) : undefined}
            {#if other && board.halves}
              <div class="pair">
                <span class="num">{key}</span>
                <button class="half" class:sel={here?.key === c.key} class:away={!heard(c.hz)} onclick={() => onTune(c.hz)} title={title(c)}>{board.halves[0]}{@render bar(c)}</button>
                <button class="half" class:sel={here?.key === other.key} class:away={!heard(other.hz)} onclick={() => onTune(other.hz)} title={title(other)}>{board.halves[1]}{@render bar(other)}</button>
              </div>
            {:else}
              <button class="tile" class:worded={c.word} class:rpt={c.dot} class:sel={here?.key === c.key} class:away={!heard(c.hz)} onclick={() => onTune(c.hz)} title={title(c)}>
                {key}{#if c.word}<small>{c.word}</small>{/if}
                {@render bar(c)}
              </button>
            {/if}
          {/each}
        </div>
      </div>
    {/each}
    <div class="legend">
      <span><i class="busy"></i>on air</span>
      {#if hasDot}<span><i class="dotkey"></i>repeater channel</span>{/if}
      {#if anyAway}<span class="faded">faded: other side, not heard</span>{/if}
    </div>
    <p class="detail">
      {#if here}
        <b>{here.name ?? here.key} · {fmtMHz(here.hz, 4)}</b>
        {here.use}
        {#if twin && board.twinLinks}
          <button class="link" onclick={() => onTune(twin.hz)}>{here.second ? board.twinLinks[1] : `${board.twinLinks[0]} ${fmtMHz(twin.hz, 4)}`}</button>
        {/if}
      {:else}
        {board.hint}
      {/if}
    </p>
  </div>
</div>

<style>
  .board {
    flex: 1 1 0;
    min-height: 0;
    container-type: size;
    overflow-y: auto;
  }
  /* a container can't query its own size, so the spacing that changes lives one level down */
  .inner {
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .strip {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .lbl {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 11px;
    color: var(--fg-dim);
  }
  .lbl b {
    color: var(--fg);
    font-weight: 600;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 4px;
  }
  .tile {
    position: relative;
    padding: 7px 0 10px;
    border: 1px solid var(--muted);
    border-radius: 6px;
    background: var(--bg-light);
    color: var(--fg);
    font: 600 14px var(--mono);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  .tile:hover,
  .half:hover {
    border-color: var(--fg-dim);
  }
  .tile.worded,
  .pair {
    grid-column: span 2;
  }
  .tile small {
    display: block;
    margin-top: 1px;
    font: 500 10px var(--sans);
    color: var(--fg-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--muted);
    border-radius: 6px;
    background: var(--bg-light);
  }
  .num {
    grid-column: 1 / 3;
    text-align: center;
    font: 600 14px var(--mono);
    font-variant-numeric: tabular-nums;
    color: var(--fg);
  }
  .half {
    position: relative;
    padding: 1px 0 7px;
    border: 1px solid transparent;
    border-radius: 4px;
    background: none;
    color: var(--fg-dim);
    font-size: 10px;
    cursor: pointer;
  }
  .half.sel {
    border-color: var(--pane, var(--hue));
    background: color-mix(in srgb, var(--pane, var(--hue)) 18%, var(--bg-light));
    color: var(--fg-bright);
  }
  .half.away {
    opacity: 0.5;
  }
  .half .bar {
    left: 3px;
    right: 3px;
    bottom: 2px;
    height: 2px;
  }
  .tile.sel {
    border-color: var(--pane, var(--hue));
    background: color-mix(in srgb, var(--pane, var(--hue)) 18%, var(--bg-light));
    color: var(--fg-bright);
  }
  .tile.away {
    opacity: 0.5;
  }
  .tile.rpt::after {
    content: '';
    position: absolute;
    top: 4px;
    right: 4px;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--orange);
  }
  .bar {
    position: absolute;
    left: 5px;
    right: 5px;
    bottom: 4px;
    height: 3px;
    border-radius: 2px;
    background: var(--muted);
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    background: var(--green);
    transform-origin: left;
    transition: transform 0.3s;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    font-size: 11px;
    color: var(--fg-dim);
  }
  .legend i {
    display: inline-block;
    vertical-align: middle;
    margin-right: 5px;
  }
  .busy {
    width: 12px;
    height: 3px;
    border-radius: 2px;
    background: var(--green);
  }
  .dotkey {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--orange);
  }
  .faded {
    opacity: 0.7;
  }
  .detail {
    margin: 0;
    padding-top: 10px;
    border-top: 1px solid var(--muted);
    font-size: 12px;
    line-height: 1.5;
    color: var(--fg-dim);
  }
  .detail b {
    color: var(--fg);
    font-family: var(--mono);
    font-weight: 600;
    margin-right: 4px;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--pane, var(--hue));
    font: inherit;
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }
  @container (max-height: 300px) {
    .legend {
      display: none;
    }
  }
  @container (max-height: 260px) {
    .inner {
      gap: 6px;
    }
    .lbl {
      display: none;
    }
    .detail {
      padding-top: 6px;
    }
  }
  @container (max-height: 190px) {
    .tile {
      padding: 3px 0 7px;
      font-size: 12px;
    }
    .detail {
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
  }
  @container (max-height: 150px) {
    .detail {
      display: none;
    }
    .inner {
      padding-block: 6px;
    }
  }
  @container (max-height: 110px) {
    .tile small {
      display: none;
    }
    .inner {
      gap: 3px;
    }
    .tiles {
      gap: 2px;
    }
    .tile {
      padding: 1px 0 5px;
      border-radius: 4px;
    }
    .bar {
      bottom: 2px;
      height: 2px;
    }
    .tile.rpt::after {
      top: 2px;
      right: 2px;
      width: 4px;
      height: 4px;
    }
  }
</style>
