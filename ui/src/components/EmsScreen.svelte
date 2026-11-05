<script lang="ts">
  // EMS for someone who has never heard of trunking: the three steps the receiver takes, one sentence on whether
  // this tower is worth waiting on, the tower as a board of channels, ten minutes of calls, then the talkgroups.
  // Covers `main` on the EMS bands; App owns the radio. Mockup 2026-11-05: https://claude.ai/artifact/SNURAma44ZLDBp4hdziFgk
  import Icon from './Icon.svelte';
  import Steps, { type Step } from './Steps.svelte';
  import Talkgroups from './Talkgroups.svelte';
  import type { P25, P25Talkgroup } from '../lib/api';
  import { LINE_SECS, voiceHeard, type LookLine } from '../lib/ems';
  import { WINDOW_MIN, channelBoard, minuteBars, sinceSay, steps, verdict, type Picks } from '../lib/p25';
  import { fmtMHz, type Saved } from '../lib/radio';
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
    /** The look's kept lines, newest first: where plain voice is found. */
    lines: LookLine[];
    /** Named frequencies, for the national interoperability channels. */
    named: Saved[];
    /** On a plain-FM band: what is tuned and whether the squelch is open. Null on the trunking bands. */
    analog: { tuned: number; open: boolean | null; lo: number; hi: number } | null;
    onListen: (hz: number) => void;
    onTowers: () => void;
  }
  let { p25, picks, lockSecs, rows, fold, otherBand, onHold, onSkip, onOtherBand, onSeeRadio, lines, named, analog, onListen, onTowers }: Props = $props();
  let voices = $derived(voiceHeard(lines));
  /** A name within half a channel of the frequency. */
  const nameOf = (hz: number) => named.find((n) => Math.abs(n.mhz * 1e6 - hz) <= 7_000)?.name ?? null;
  const agoSay = (lineAgo: number) => (lineAgo === 0 ? 'in the last look' : sinceSay(lineAgo * LINE_SECS));
  let stepList = $derived(steps(p25, lockSecs));
  const STEP_ICONS = ['search', 'tower', 'headphones'];
  const STEP_WORDS_NOW = ['Finding a tower', 'Reading the tower', 'Hearing calls'];
  /** A few words per step; the long sentence the library builds is the step's tooltip. */
  let stepItems: Step[] = $derived(
    stepList.map((s, i) => ({
      icon: STEP_ICONS[i],
      label: s.state === 'done' ? ['Found a tower', 'Read the tower', 'Hearing calls'][i] : STEP_WORDS_NOW[i],
      state: s.state,
      title: `${s.say}. ${i === 0 ? GLOSS.control : i === 1 ? GLOSS.tower : GLOSS.calls}`,
    })),
  );
  let said = $derived(verdict(p25, picks, lockSecs));
  let board = $derived(p25 ? channelBoard(p25) : []);
  let bars = $derived(p25 ? minuteBars(p25.groups) : []);
  let tallest = $derived(Math.max(1, ...bars.map((b) => b.playable + b.locked)));
  const BAR_H = 36;
  const GLOSS = {
    control: 'The control channel: always on, data only, about 30 messages a second. The receiver sits on it and reads which voice channel each call gets.',
    tower: 'A tower (site) of the agency’s network (system). Each tower has its own control channel; the number is the tower’s inside its region.',
    calls: 'A call: one talkgroup keying up for a few seconds on one voice channel. A talkgroup is a group of radios that hear each other, a dispatch desk and its units.',
    plain: 'Plain voice: ordinary narrow FM with no tower in between, which many counties and volunteer departments still use. Named ones are the national interoperability channels, the same everywhere.',
  };
</script>

<div class="ems">
  {#if analog}
    <Steps steps={[{ icon: 'headphones', label: 'Listening to plain voice', state: 'now', title: `${fmtMHz(analog.tuned)} MHz${nameOf(analog.tuned) ? ` · ${nameOf(analog.tuned)}` : ''}. ${GLOSS.plain}` }]} />
    <p class="verdict">
      {analog.open ? 'Someone is talking.' : 'Quiet. Plain FM dispatch comes in short bursts; the squelch opens when someone keys up.'}
      <button class="act" onclick={onTowers} title="Back to the trunked towers">Back to the towers</button>
    </p>
  {:else}
    <Steps steps={stepItems} label="Where the receiver is" />
    <p class="verdict">
      {said.say}
      {#if said.action === 'other-band' && otherBand}
        <button class="act" onclick={onOtherBand} title="Start the search again on {otherBand}, at its own start">Try {otherBand}</button>
      {/if}
    </p>
  {/if}

  <div class="body">
    <div class="left">
      {#if !analog && !board.length}
        <div class="state" title={said.say}>
          <Icon name={p25 && p25.state !== 'searching' ? 'tower' : 'search'} size={44} />
          <span>{p25 && p25.state !== 'searching' ? 'Tower found' : 'Finding a tower'}</span>
        </div>
      {/if}
      {#if !analog && board.length}
        <h2 class="sect" title={GLOSS.tower}>This tower’s channels <span class="dim">· lit while a call is on</span></h2>
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
      {#if !analog && p25 && p25.state !== 'searching'}
        <h2 class="sect" title={GLOSS.calls}>Calls <span class="dim">· last {WINDOW_MIN} min</span></h2>
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
      <h2 class="sect" title="{GLOSS.plain} The whole band is looked at every {LINE_SECS} s.">Plain voice heard</h2>
      {#if voices.length}
        <div class="voices">
          {#each voices as v (v.hz)}
            <button class="voice" class:sel={!!analog && Math.abs(analog.tuned - v.hz) < 7_000} onclick={() => onListen(v.hz)} title="Listen here as plain FM">
              <span class="ic"><Icon name="play" size={13} /></span>
              <span class="f">{fmtMHz(v.hz)} MHz</span>
              <span class="nm">{nameOf(v.hz) ?? 'unnamed'}</span>
              <span class="dim">on in {v.seen} of {v.of} looks · {agoSay(v.ago)}</span>
            </button>
          {/each}
        </div>
      {:else}
        <div class="state small" title="Plain FM dispatch is rare near big cities; towns and counties still use it.">
          <Icon name="quiet" size={40} />
          <span>{lines.length < 3 ? `Looking: ${lines.length} of 3 looks` : 'No plain voice heard'}</span>
        </div>
      {/if}
    </div>
    <div class="side">
      {#if p25 && !analog}
        <Talkgroups {p25} {picks} {rows} {fold} {onHold} {onSkip} />
      {:else if analog}
        <h2 class="sect" title={GLOSS.plain}>Named channels on this band</h2>
        <div class="voices">
          {#each named.filter((n) => n.mhz * 1e6 >= analog.lo && n.mhz * 1e6 <= analog.hi) as n (n.mhz)}
            <button class="voice" class:sel={Math.abs(analog.tuned - n.mhz * 1e6) < 7_000} onclick={() => onListen(n.mhz * 1e6)} title="Listen here as plain FM">
              <span class="ic"><Icon name="play" size={13} /></span>
              <span class="f">{fmtMHz(n.mhz * 1e6)} MHz</span>
              <span class="nm">{n.name}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
  <p class="foot"><button class="link" onclick={onSeeRadio} title="The waterfall and the whole band. A click there tunes by hand and leaves the tower.">See the radio</button> · the waterfall</p>
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
    border-radius: var(--r-sm);
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
  .dim {
    font-weight: 400;
  }
  .sect {
    margin: 10px 0 6px;
  }
  .state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 18px 0;
    color: var(--fg-dim);
    opacity: 0.8;
    font-size: 13px;
  }
  .state.small {
    padding: 8px 0;
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
    border-radius: var(--r);
    padding: 7px 9px;
    font-size: 12px;
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
    font-size: 11px;
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
  .voices {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .voice {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 9px;
    border: 1px solid var(--muted);
    border-radius: var(--r);
    background: var(--bg-light);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    min-width: 0;
  }
  .voice.sel {
    border-color: var(--accent);
  }
  .voice .f {
    font-family: var(--mono);
    font-size: 12px;
  }
  .voice .nm {
    font-weight: 600;
    flex: 1;
    min-width: 0;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .voice .dim {
    font-size: 12px;
    white-space: nowrap;
  }
  .small {
    margin: 0;
    font-size: 12px;
    color: var(--fg-dim);
    max-width: 70ch;
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
