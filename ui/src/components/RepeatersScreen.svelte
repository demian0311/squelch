<script lang="ts">
  // Repeaters for someone who has never tuned one: the steps the receiver takes, one sentence on what plays and
  // what a click does, then every repeater in reach as a row (FM or DMR, what it does now) with the map beside.
  // Every row inside the capture is heard at once; the held one plays, else whoever talks first. Covers `main`
  // on the ham bands; App owns the radio. Mockup 2026-11-05: https://claude.ai/artifact/P4XjQ1zc3Bg5BS71oJxeFM
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  import Steps, { type Step } from './Steps.svelte';
  import { fmtMHz } from '../lib/radio';
  import type { RepRow, RepStep } from '../lib/repscreen';
  interface Props {
    rows: RepRow[];
    steps: RepStep[];
    verdict: string;
    /** A row clicked: hold it, let it go, or move the capture onto it. */
    onRow: (hz: number) => void;
    onSeeRadio: () => void;
    /** The map, drawn by App. */
    map: Snippet;
  }
  let { rows, steps, verdict, onRow, onSeeRadio, map }: Props = $props();
  const STEP_ICONS = ['pin', 'radio', 'headphones'];
  const STEP_WHY = ['Repeaters in reach, from RepeaterBook and a guess of how far this band carries.', 'Every repeater inside the radio’s 2.4 MHz capture is heard at the same time, like a scanner with no gaps.', 'The receiver plays the held row, else whoever talks first.'];
  let stepList: Step[] = $derived(steps.map((s, i) => ({ icon: STEP_ICONS[i] ?? 'radio', label: s.say, state: s.state, title: STEP_WHY[i] })));
  /** A row's state as a glyph and a few words; the sentence behind it is the row's second line. */
  const STATE_ICON = { playing: 'speaker', talking: 'zap', quiet: 'quiet', away: 'arrowRight' } as const;
  const stateWords = (r: RepRow) => (r.state === 'away' ? 'Outside capture' : r.say);
  /** The second line: the click hint lives in the row's tooltip. */
  const detailOf = (r: RepRow) => r.detail.replace(/^click to move there · /, '');
  let near = $derived(rows.filter((r) => r.p.reach));
  let far = $derived(rows.filter((r) => !r.p.reach));
  const GLOSS = {
    dmr: 'DMR: digital voice. The repeater is silent between calls, so a quiet row is normal; a call fills it and plays. Two conversations fit at once, one per slot.',
    fm: 'Plain FM voice. A carrier on the frequency means someone is keyed up.',
    hold: 'Click a row to stay on it: only it plays. Click it again and the radio goes back to playing whoever talks first.',
    far: 'Probably too far for this band’s usual range, from the distance alone; a repeater on a mountain or a tall tower may still come in.',
  };
</script>

<div class="rep">
  <Steps steps={stepList} />
  <p class="verdict">{verdict}</p>

  <div class="body">
    <div class="rows" role="list" title={GLOSS.hold}>
      {#snippet row(r: RepRow)}
        <button class="row {r.state}" class:held={r.held} class:dmr={r.dmr} class:far={!r.p.reach} role="listitem" onclick={() => onRow(r.p.hz)} title={r.held ? 'Held: only this one plays. Click to let go.' : r.heard ? 'Click to stay on this repeater' : 'Click to move the radio onto this repeater'}>
          <span class="who"><b>{r.p.call}</b><span class="city">{r.p.city}</span></span>
          <span class="num">{r.p.km < 10 ? r.p.km.toFixed(1) : Math.round(r.p.km)} km</span>
          <span class="num">{fmtMHz(r.p.hz, 4)}</span>
          <span class="chip" class:isdmr={r.dmr} title={r.dmr ? GLOSS.dmr : GLOSS.fm}>{r.dmr ? 'DMR' : 'FM'}</span>
          <span class="now">
            <span class="say">
              <span class="ic"><Icon name={STATE_ICON[r.state]} size={16} /></span>{stateWords(r)}{#if r.held}<span class="ic" title="Held"><Icon name="lock" size={12} /></span>{/if}
            </span>
            <small>{#if r.state === 'quiet'}<Icon name="clock" size={11} /> {/if}{detailOf(r)}</small>
          </span>
        </button>
      {/snippet}
      {#each near as r (r.p.id)}{@render row(r)}{/each}
      {#if far.length}
        <h3 class="sect divider" title={GLOSS.far}>Probably too far <span class="dim">· {far.length}</span></h3>
        {#each far as r (r.p.id)}{@render row(r)}{/each}
      {/if}
      {#if !rows.length}
        <p class="empty">No repeater this radio can play is listed near you on this band.</p>
      {/if}
    </div>
    <div class="map">{@render map()}</div>
  </div>

  <p class="foot">
    <span title="Every row inside the capture is heard at once. The held row plays; with none held, whoever talks first.">All rows in the capture play at once.</span>
    <button class="act" onclick={onSeeRadio} title="The waterfall and the whole band">See the radio</button>
  </p>
</div>

<style>
  .rep {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 18px;
    background: var(--bg);
    overflow: hidden;
    container-type: inline-size;
  }
  .verdict {
    margin: 0;
    font-size: 15px;
    max-width: 90ch;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
    gap: 14px;
  }
  @container (max-width: 820px) {
    .body {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(0, 3fr) minmax(200px, 2fr);
    }
  }
  .rows {
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(110px, 1.3fr) 56px 92px 44px minmax(0, 3fr);
    gap: 10px;
    align-items: center;
    padding: 7px 10px;
    border-radius: var(--r);
    border: 1px solid var(--muted);
    background: var(--bg-light);
    color: var(--fg);
    text-align: left;
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .row:hover {
    border-color: var(--fg-dim);
  }
  .row.talking,
  .row.playing {
    background: color-mix(in srgb, var(--green) 12%, var(--bg-light));
  }
  .row.playing {
    border-color: var(--green);
  }
  .row.held {
    border-color: var(--accent);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .row.far {
    opacity: 0.6;
  }
  .row.away .now {
    color: var(--fg-dim);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .who b {
    font-weight: 600;
  }
  .city {
    color: var(--fg-dim);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .num {
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    color: var(--fg-dim);
    font-size: 12px;
  }
  .chip {
    font-family: var(--mono);
    font-size: 11px;
    padding: 1px 6px;
    border-radius: var(--r-sm);
    text-align: center;
    background: var(--green);
    color: var(--bg);
  }
  .chip.isdmr {
    background: var(--blue);
  }
  .now {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .say {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .ic {
    display: inline-flex;
  }
  .now small {
    color: var(--fg-dim);
    font-size: 12px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .divider {
    margin: 8px 2px 2px;
  }
  .dim {
    font-weight: 400;
  }
  .empty {
    color: var(--fg-dim);
    margin: 6px 2px;
  }
  .map {
    position: relative;
    min-height: 0;
    overflow: hidden;
    border-radius: var(--r);
    background: var(--bg-darker);
  }
  .foot {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    margin: 0;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .act {
    font-family: var(--mono);
    font-size: 12px;
    border: 1px solid var(--accent);
    color: var(--accent);
    background: none;
    border-radius: var(--r-sm);
    padding: 3px 10px;
    cursor: pointer;
    white-space: nowrap;
  }
  @container (max-width: 600px) {
    .row {
      grid-template-columns: minmax(90px, 1fr) 44px minmax(0, 2fr);
    }
    .row .num:nth-child(3),
    .row .chip {
      display: none;
    }
  }
</style>
