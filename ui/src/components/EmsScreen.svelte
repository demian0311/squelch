<script lang="ts">
  // EMS for someone who has never heard of trunking: the three steps the receiver takes, one sentence on whether
  // this tower is worth waiting on, the tower as a board of channels, ten minutes of calls, then the talkgroups.
  // Covers `main` on the EMS bands; App owns the radio. Mockup 2026-11-05: https://claude.ai/artifact/SNURAma44ZLDBp4hdziFgk
  import Icon from './Icon.svelte';
  import Talkgroups from './Talkgroups.svelte';
  import type { P25, P25Talkgroup } from '../lib/api';
  import { WINDOW_MIN, channelBoard, minuteBars, steps, verdict, type Picks } from '../lib/p25';
  import { fmtMHz } from '../lib/radio';
  interface Props {
    p25: P25 | null;
    picks: Picks;
    /** How long the receiver has sat on this tower. */
    lockSecs: number;
    rows: P25Talkgroup[];
    fold: { encrypted: string | null; tdma: string | null };
    /** The other EMS band's name, when there is one to try. */
    otherBand: string | null;
    onHold: (group: number) => void;
    onSkip: (group: number) => void;
    onOtherBand: () => void;
    onSeeRadio: () => void;
  }
  let { p25, picks, lockSecs, rows, fold, otherBand, onHold, onSkip, onOtherBand, onSeeRadio }: Props = $props();
  let stepList = $derived(steps(p25, lockSecs));
  let said = $derived(verdict(p25, picks, lockSecs));
  let board = $derived(p25 ? channelBoard(p25) : []);
  let bars = $derived(p25 ? minuteBars(p25.groups) : []);
  let tallest = $derived(Math.max(1, ...bars.map((b) => b.playable + b.locked)));
  const BAR_H = 36;
  const GLOSS = {
    control: 'The control channel: always on, data only, about 30 messages a second. The receiver sits on it and reads which voice channel each call gets.',
    tower: 'A tower (site) of the agency’s network (system). Each tower has its own control channel; the number is the tower’s inside its region.',
    calls: 'A call: one talkgroup keying up for a few seconds on one voice channel. A talkgroup is a group of radios that hear each other, a dispatch desk and its units.',
  };
</script>

<div class="ems">
  <section class="steps" aria-label="Where the receiver is">
    {#each stepList as s, i (s.n)}
      {#if i}<span class="arrow">→</span>{/if}
      <span class="step" class:done={s.state === 'done'} class:now={s.state === 'now'} title={s.n === 1 ? GLOSS.control : s.n === 2 ? GLOSS.tower : GLOSS.calls}>
        <b>{s.state === 'done' ? '✓' : s.n}</b>{s.say}
      </span>
    {/each}
  </section>
  <p class="verdict">
    {said.say}
    {#if said.action === 'other-band' && otherBand}
      <button class="act" onclick={onOtherBand} title="Start the search again on {otherBand}, at its own start">Try {otherBand}</button>
    {/if}
  </p>

  <div class="body">
    <div class="left">
      {#if board.length}
        <h2 title={GLOSS.tower}>This tower's channels <span class="dim">· lit while a call is on</span></h2>
        <div class="board">
          {#each board as c (c.hz)}
            <div class="ch" class:cc={c.kind === 'control'} class:lit={c.lit && c.kind === 'voice'} class:lock={c.kind === 'voice' && !c.playable} title={c.kind === 'control' ? GLOSS.control : c.playable ? 'A voice channel this tower used; the receiver plays a call here when it is in the clear' : 'A voice channel whose last call was encrypted or Phase 2'}>
              {#if c.kind === 'control'}
                <span class="what">Control</span>
                <span class="f">{fmtMHz(c.hz)} · {p25?.messages_per_sec.toFixed(0) ?? 0} msg/s</span>
              {:else}
                <span class="what">
                  {#if c.playing}<span class="ic"><Icon name="speaker" size={13} /></span>{:else if c.encrypted}<span class="ic"><Icon name="lock" size={13} /></span>{:else if !c.playable}<span class="ic"><Icon name="mute" size={13} /></span>{/if}
                  Talkgroup {c.group}
                </span>
                <span class="f">{fmtMHz(c.hz)} · {c.playing ? 'playing' : c.lit ? 'call on' : c.encrypted ? 'encrypted' : c.playable ? 'quiet' : 'Phase 2'}</span>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
      {#if p25 && p25.state !== 'searching'}
        <h2 title={GLOSS.calls}>Calls <span class="dim">· last {WINDOW_MIN} min · a column a minute</span></h2>
        <div class="timeline" role="img" aria-label="Calls per minute: those you can hear and those that are locked">
          {#each bars as b, i (i)}
            <div class="col" title="{b.playable} you can hear, {b.locked} locked">
              <i class="locked" style:height="{(BAR_H * b.locked) / tallest}px"></i>
              <i class="play" style:height="{(BAR_H * b.playable) / tallest}px"></i>
            </div>
          {/each}
        </div>
        <div class="axis"><span>−{WINDOW_MIN} min</span><span class="legend"><i class="play"></i> you can hear <i class="locked"></i> encrypted or Phase 2</span><span>now</span></div>
      {/if}
    </div>
    <div class="side">
      {#if p25}
        <Talkgroups {p25} {picks} {rows} {fold} {onHold} {onSkip} />
      {/if}
    </div>
  </div>
  <p class="foot"><button class="link" onclick={onSeeRadio}>See the radio</button> · the waterfall and the whole band. A click there tunes by hand and leaves the tower.</p>
</div>

<style>
  .ems {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 18px;
    background: var(--bg);
    overflow: auto;
    container-type: inline-size;
  }
  .steps {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 0;
  }
  .step {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 4px 11px 4px 6px;
    border-radius: 999px;
    border: 1px solid var(--muted);
    font-size: 13px;
    white-space: nowrap;
    color: var(--fg-dim);
  }
  .step b {
    display: inline-flex;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    border: 1.5px solid var(--fg-dim);
  }
  .step.done {
    color: var(--fg);
  }
  .step.done b {
    background: var(--green);
    border-color: var(--green);
    color: var(--bg);
  }
  .step.now {
    color: var(--fg);
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .step.now b {
    border-color: var(--accent);
    color: var(--accent);
  }
  .arrow {
    color: var(--fg-dim);
    padding: 0 7px;
  }
  .verdict {
    margin: 0;
    font-size: 15px;
    max-width: 80ch;
  }
  .act {
    font-family: var(--mono);
    font-size: 12px;
    border: 1px solid var(--accent);
    color: var(--accent);
    background: none;
    border-radius: 5px;
    padding: 1px 8px;
    margin-left: 6px;
    cursor: pointer;
    white-space: nowrap;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(280px, 380px);
    gap: 18px;
  }
  @container (max-width: 760px) {
    .body {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  h2 {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--fg-dim);
    margin: 10px 0 6px;
  }
  .dim {
    color: var(--fg-dim);
    font-weight: 400;
    text-transform: none;
    letter-spacing: 0;
  }
  .board {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(128px, 1fr));
    gap: 8px;
  }
  .ch {
    display: flex;
    flex-direction: column;
    gap: 3px;
    border: 1px solid var(--muted);
    border-radius: 7px;
    padding: 7px 9px;
    font-size: 12.5px;
    background: var(--bg-light);
    min-height: 44px;
  }
  .ch .what {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-weight: 600;
  }
  .ch .f {
    font-family: var(--mono);
    font-size: 11.5px;
    color: var(--fg-dim);
  }
  .ch.cc {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .ch.lit {
    border-color: var(--green);
  }
  .ch.lock {
    color: var(--fg-dim);
  }
  .ch.lock.lit {
    border-color: var(--fg-dim);
  }
  .ic {
    display: inline-flex;
  }
  .timeline {
    display: grid;
    grid-template-columns: repeat(10, 1fr);
    gap: 4px;
    height: 36px;
    align-items: end;
  }
  .col {
    display: flex;
    flex-direction: column-reverse;
    gap: 1px;
    border-bottom: 1px solid var(--muted);
    min-height: 100%;
    justify-content: start;
  }
  .col i,
  .legend i {
    display: block;
    border-radius: 2px 2px 0 0;
  }
  .col i.play,
  .legend i.play {
    background: var(--green);
  }
  .col i.locked,
  .legend i.locked {
    background: var(--muted);
  }
  .axis {
    display: flex;
    justify-content: space-between;
    font-family: var(--mono);
    font-size: 11px;
    color: var(--fg-dim);
    margin-top: 3px;
  }
  .legend {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-family: var(--sans);
  }
  .legend i {
    width: 10px;
    height: 10px;
    border-radius: 2px;
  }
  .side {
    min-width: 0;
  }
  .foot {
    margin: 0;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .link {
    background: none;
    border: 0;
    padding: 0;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
  }
</style>
