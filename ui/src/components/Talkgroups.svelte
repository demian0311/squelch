<script lang="ts">
  // The talkgroups this receiver can hear, one row each, and a foot line counting what it never will.
  // The row is the control: click holds the talkgroup (only it plays), its chip skips it (never plays).
  import Icon from './Icon.svelte';
  import type { P25, P25Talkgroup } from '../lib/api';
  import { WINDOW_MIN, barPx, sinceSay, type Picks } from '../lib/p25';
  interface Props {
    p25: P25;
    picks: Picks;
    rows: P25Talkgroup[];
    fold: { encrypted: string | null; tdma: string | null };
    onHold: (group: number) => void;
    onSkip: (group: number) => void;
  }
  let { p25, picks, rows, fold, onHold, onSkip }: Props = $props();
</script>

<h2 class="sect">Talkgroups <span class="dim">· last {WINDOW_MIN} min</span></h2>
{#each rows as r (r.group)}
  {@const live = p25.call?.group === r.group}
  {@const held = picks.hold === r.group}
  {@const skipped = picks.skip.includes(r.group)}
  {@const max = Math.max(...r.minutes)}
  <div class="item tg" class:sel={live} class:quiet={skipped} role="button" tabindex="0" title={held ? 'Only this talkgroup plays. Click to let every talkgroup play.' : 'Click to hear only this talkgroup'} onclick={() => onHold(r.group)} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onHold(r.group); } }}>
    <span class="name gl">
      {#if live}<span class="ic" title="Playing"><Icon name="speaker" size={14} /></span>
      {:else if r.emergency}<span class="ic alarm" title="An emergency call in the last {WINDOW_MIN} min"><Icon name="siren" size={14} /></span>{/if}
      <span>Talkgroup {r.group}</span>
      <span class="bars" title="{r.calls} {r.calls === 1 ? 'call' : 'calls'} in the last {WINDOW_MIN} min, a bar a minute">{#each r.minutes as m, i (i)}<i class:q={!m} style:height="{barPx(m, max)}px"></i>{/each}</span>
      <span class="dim age">{live ? 'playing' : skipped ? 'skipped' : sinceSay(p25.now_secs - r.last_secs)}</span>
      {#if held}
        <button class="act on" title="Only this talkgroup plays. Click to let every talkgroup play." onclick={(e) => { e.stopPropagation(); onHold(r.group); }}>held</button>
      {:else}
        <button class="act" class:on={skipped} title={skipped ? 'Never plays. Click to let it play again.' : 'Never play this talkgroup'} onclick={(e) => { e.stopPropagation(); onSkip(r.group); }}>skip</button>
      {/if}
    </span>
  </div>
{:else}
  <p class="empty">{p25.groups.length ? 'Nothing to hear yet: every call so far was encrypted or Phase 2.' : 'No calls yet. Busy sites assign several a minute; quiet ones can go minutes between.'}</p>
{/each}
{#if fold.encrypted || fold.tdma}
  <p class="foot dim">
    {#if fold.encrypted}<span class="ic" title="Encrypted: no scanner can hear it"><Icon name="lock" size={13} /></span><span>{fold.encrypted}</span>{/if}
    {#if fold.encrypted && fold.tdma}<span>·</span>{/if}
    {#if fold.tdma}<span class="ic" title="Phase 2: two calls share one channel in turns, which this receiver cannot play yet"><Icon name="mute" size={13} /></span><span>{fold.tdma}</span>{/if}
    <span>This receiver cannot play them.</span>
  </p>
{/if}

<style>
  h2 {
    font-size: 12px;
    font-weight: 600;
    color: var(--fg-dim);
    margin: 0 0 6px;
  }
  .dim {
    color: var(--fg-dim);
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 8px 12px;
    border-radius: var(--r);
    border: 1px solid var(--muted);
    background: var(--bg-light);
    text-align: left;
    margin-bottom: 6px;
    font-size: 13px;
  }
  .item.sel {
    border-color: var(--accent);
  }
  .item.quiet {
    opacity: 0.55;
  }
  .item.tg {
    cursor: pointer;
  }
  .name {
    font-weight: 600;
  }
  .gl {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .ic {
    display: inline-flex;
  }
  .alarm {
    color: var(--red);
  }
  .age {
    margin-left: auto;
    font-weight: 400;
  }
  .bars {
    display: inline-flex;
    gap: 1.5px;
    align-items: flex-end;
    height: 14px;
    margin-left: 4px;
  }
  .bars i {
    display: block;
    width: 4px;
    background: var(--accent);
    border-radius: 1px;
  }
  .bars i.q {
    background: var(--muted);
  }
  .act {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--fg-dim);
    background: none;
    border: 1px solid var(--muted);
    border-radius: var(--r-sm);
    padding: 1px 6px;
    cursor: pointer;
  }
  .act.on {
    color: var(--accent);
    border-color: var(--accent);
  }
  .empty {
    font-size: 12px;
    color: var(--fg-dim);
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 6px;
    align-items: center;
    font-size: 12px;
    margin: 6px 0 0;
    padding: 8px 12px 2px;
    border-top: 1px dashed var(--muted);
  }
</style>
