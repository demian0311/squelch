<script lang="ts">
  // FRS and GMRS as one panel: 22 tiles in three strips by what the channels are for, a bar on each while it is on air.
  // Repeater inputs are not tiles: the line under the strips tunes the tuned repeater's input and back.
  import { STRIPS, WALKIE, isRepeater, sideOf, stripOf, walkieAt, walkieByKey, type Side } from '../lib/walkie';
  import { fmtMHz } from '../lib/radio';

  interface Props {
    /** Strongest signal per channel key, dB above the floor. */
    live: Map<string, number>;
    tuned: number;
    /** Sides in the capture now; channels on the other side are not heard. */
    sides: Side[];
    /** dB that fills a bar. */
    fullDb: number;
    onTune: (hz: number) => void;
  }
  let { live, tuned, sides, fullDb, onTune }: Props = $props();

  let here = $derived(walkieAt(tuned));
  const tiles = (from: number, to: number) => WALKIE.filter((c) => !c.input && c.n >= from && c.n <= to);
  const heard = (hz: number) => sides.includes(sideOf(hz));
  const pct = (key: string) => Math.max(8, Math.min(100, ((live.get(key) ?? 0) / fullDb) * 100));

  function use(n: number, input: boolean): string {
    if (input) return `People talking into the channel ${n} repeater, heard before it repeats them.`;
    if (isRepeater(n)) return 'Repeater output, or people talking directly.';
    if (n >= 8) return 'Short range: hand-helds at 0.5 W only.';
    return 'Everyday channel.';
  }
</script>

<div class="walkie">
  {#each STRIPS as s (s.name)}
    <div class="strip">
      <div class="lbl"><span><b>{s.name}</b> {s.from}–{s.to}</span><span>{s.power}</span></div>
      <div class="tiles">
        {#each tiles(s.from, s.to) as c (c.key)}
          {@const on = live.has(c.key) && heard(c.hz)}
          <button class="tile" class:rpt={isRepeater(c.n)} class:sel={here?.key === c.key} class:away={!heard(c.hz)} onclick={() => onTune(c.hz)} title="{fmtMHz(c.hz, 4)} MHz{isRepeater(c.n) ? ' · repeater channel' : ''}{heard(c.hz) ? '' : ' · not heard from the tuned side'}">
            {c.n}
            <span class="bar"><i style:transform="scaleX({on ? pct(c.key) / 100 : 0})"></i></span>
          </button>
        {/each}
      </div>
    </div>
  {/each}
  <div class="legend">
    <span><i class="busy"></i>on air</span>
    <span><i class="dotkey"></i>repeater channel</span>
    {#if sides.length < 2}<span class="faded">faded: other side, not heard</span>{/if}
  </div>
  <p class="detail">
    {#if here}
      {@const input = walkieByKey.get(`${here.n}R`)}
      {@const out = walkieByKey.get(String(here.n))!}
      <b>{here.key} · {fmtMHz(here.hz, 4)}</b>
      {use(here.n, here.input)} {here.input ? '' : stripOf(here.n).power + '.'}
      {#if input && !here.input}
        <button class="link" onclick={() => onTune(input.hz)}>Listen to its users on {fmtMHz(input.hz, 4)}</button>
      {:else if here.input}
        <button class="link" onclick={() => onTune(out.hz)}>Back to the repeater</button>
      {/if}
      {#if sideOf(here.hz) === 467}<span class="note">Channels 1–7 and 15–22 are not heard while tuned here.</span>{/if}
    {:else}
      Click a channel to tune it. FRS and GMRS radios use the same 22 channels.
    {/if}
  </p>
</div>

<style>
  .walkie {
    flex: 1 1 0;
    min-height: 0;
    overflow-y: auto;
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
  .tile:hover {
    border-color: var(--fg-dim);
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
  .note {
    display: block;
    margin-top: 4px;
  }
</style>
