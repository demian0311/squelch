<script lang="ts">
  // When each of a band's channels was on air, from the first minute listened (at most KEEP_MIN ago) to now.
  // Blank cells: that channel's side was not in the capture. Click a row to tune.
  import { ago, spanText, type Activity, type ActivityRow, type Board } from '../lib/board';

  interface Props {
    board: Board;
    act: Activity;
    tunedKey: string | undefined;
    now: number;
    onTune: (hz: number) => void;
  }
  let { board, act, tunedKey, now, onTune }: Props = $props();

  let groups = $derived<[string, ActivityRow[]][]>([
    ...board.strips.map((s): [string, ActivityRow[]] => [s.name, s.keys.flatMap((k) => act.rows.filter((r) => r.channel.key === k))]),
    ...(board.secondName ? [[board.secondName, act.rows.filter((r) => r.channel.second)] as [string, ActivityRow[]]] : []),
  ]);
  const fill = (v: number | null) => (v === null ? 'transparent' : v ? `color-mix(in srgb, var(--green) ${Math.round(30 + v * 70)}%, var(--bg-light))` : 'var(--bg-light)');
</script>

<div class="activity">
  {#each groups as [name, rows] (name)}
    <h3>{name}</h3>
    {#each rows as r (r.channel.key)}
      <button class="row" class:sel={r.channel.key === tunedKey} onclick={() => onTune(r.channel.hz)}>
        <span class="num" class:rpt={r.channel.dot}>{r.channel.key}</span>
        <span class="cells" style:grid-template-columns="repeat({r.cells.length}, 1fr)">
          {#each r.cells as v, i (i)}<i style:background={fill(v)}></i>{/each}
        </span>
        <span class="ago">{ago(r.last, now)}</span>
      </button>
    {/each}
  {/each}
  <div class="axis"><span>{spanText(act.spanMin)} ago</span><span>now</span></div>
  <p class="note">Recorded while Squelch is open and the dongle is on that side. Kept for 6 hours.</p>
</div>

<style>
  .activity {
    flex: 1 1 0;
    min-height: 0;
    overflow-y: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  h3 {
    margin: 10px 0 3px 5ch;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--fg-dim);
  }
  h3:first-child {
    margin-top: 0;
  }
  .row {
    display: grid;
    grid-template-columns: 4ch 1fr 7ch;
    gap: 8px;
    align-items: center;
    min-height: 14px;
    padding: 1px 4px;
    border: 0;
    border-radius: 3px;
    background: transparent;
    color: var(--fg);
    cursor: pointer;
  }
  .row:hover {
    background: var(--bg-light);
  }
  .row.sel {
    background: color-mix(in srgb, var(--pane, var(--hue)) 16%, transparent);
  }
  .num {
    font: 600 11px var(--mono);
    text-align: right;
  }
  .num.rpt {
    color: var(--orange);
  }
  .cells {
    display: grid;
    gap: 1px;
    height: 9px;
  }
  .cells i {
    border-radius: 1px;
  }
  .ago {
    font: 10px var(--mono);
    color: var(--fg-dim);
    text-align: right;
  }
  .axis {
    display: flex;
    justify-content: space-between;
    margin: 6px calc(7ch + 12px) 0 calc(4ch + 12px);
    font: 10px var(--mono);
    color: var(--fg-dim);
  }
  .note {
    margin: 10px 0 0;
    font-size: 11px;
    color: var(--fg-dim);
  }
</style>
