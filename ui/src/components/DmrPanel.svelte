<script lang="ts">
  // DMR mode: the two time slots of the tuned repeater, what each carries, and the calls that ended.
  // One slot plays at a time; encrypted calls are counted and muted.
  import Icon from './Icon.svelte';
  import type { Dmr } from '../lib/api';
  import { agoSay, callSay, callState, lengthSay } from '../lib/dmr';
  interface Props {
    dmr: Dmr;
    /** Tune the same frequency as plain FM: a repeater listed DMR may be on FM right now. */
    onFm: () => void;
  }
  let { dmr, onFm }: Props = $props();
  const SOURCE: Record<string, string> = { repeater: 'from a repeater', radio: 'from a radio, not a repeater', direct: 'radio to radio, direct mode' };
  const GLOSS = {
    talkgroup: 'A talkgroup is a channel number radios tune to. One repeater carries many, two at a time, one per slot.',
    slot: 'DMR splits each second into two time slots, so one repeater carries two conversations at once.',
    radio: 'Each DMR radio has a number, its DMR ID. Names come from the RadioID list, which this receiver does not carry.',
    colour: 'The colour code is a number 0–15 a repeater stamps on everything it sends, so radios ignore a neighbour on the same frequency.',
  };
  let recent = $derived(dmr.recent.slice(0, 20));
</script>

<div class="head">
  {#if dmr.heard}
    <span class="name">DMR {dmr.source ? SOURCE[dmr.source] : ''}{#if dmr.colour_code !== null} · <span title={GLOSS.colour}>colour code {dmr.colour_code}</span>{/if}</span>
    <span class="dim">{dmr.bursts_per_sec.toFixed(0)} bursts/s · {dmr.slots.some((s) => s.call) ? 'one slot plays at a time, the first to carry a call' : 'quiet between calls is normal: when someone talks, a slot below fills and plays'}</span>
  {:else}
    <span class="name">{dmr.colour_code === null ? 'No DMR heard here yet' : 'DMR quiet'}</span>
    <span class="dim">A DMR repeater sends nothing between calls, so silence here is normal. One that stays silent while the waterfall shows a signal may be on plain FM. <button class="link" onclick={onFm}>Hear it as FM</button></span>
  {/if}
  {#if dmr.voice_error}<span class="dim err">{dmr.voice_error}</span>{/if}
</div>

{#each [0, 1] as i (i)}
  {@const s = dmr.slots[i]}
  {@const c = s.call}
  <div class="item" class:sel={!!c && s.playing} class:quiet={!c}>
    <span class="name gl">
      {#if c && s.playing && !c.encrypted}<span class="ic" title="Playing"><Icon name="speaker" size={14} /></span>
      {:else if c?.encrypted}<span class="ic" title="Encrypted: muted"><Icon name="lock" size={14} /></span>
      {:else if c?.emergency}<span class="ic alarm" title="An emergency call"><Icon name="siren" size={14} /></span>{/if}
      <span title={GLOSS.slot}>Slot {i + 1}</span>
      <span class="dim age">{callState(dmr, i as 0 | 1)}</span>
    </span>
    {#if c}
      <span title={c.group ? GLOSS.talkgroup : GLOSS.radio}>{callSay(c)}</span>
      <span class="dim">{lengthSay(c)} so far{c.frames ? ` · ${Math.round((100 * c.bad_frames) / c.frames)}% damaged` : ''}</span>
    {:else}
      <span class="dim">{s.calls ? `${s.calls} ${s.calls === 1 ? 'call' : 'calls'} so far` : 'No call yet'}</span>
    {/if}
  </div>
{/each}

<h2>Calls <span class="dim">· newest first</span></h2>
{#each recent as c, i (i)}
  <div class="row">
    <span class="what" title={c.group ? GLOSS.talkgroup : GLOSS.radio}>{#if c.encrypted}<span class="ic" title="Encrypted"><Icon name="lock" size={12} /></span>{/if}{#if c.emergency}<span class="ic alarm" title="Emergency"><Icon name="siren" size={12} /></span>{/if}{callSay(c)}</span>
    <span class="dim mono">slot {c.slot} · {lengthSay(c)} · {agoSay(dmr.now_secs, c)}</span>
  </div>
{:else}
  <p class="empty">No call has ended yet.</p>
{/each}
{#if dmr.encrypted_calls}<p class="empty">{dmr.encrypted_calls} encrypted {dmr.encrypted_calls === 1 ? 'call' : 'calls'} muted.</p>{/if}

<style>
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
  }
  .err {
    color: var(--red);
  }
  .mono {
    font-variant-numeric: tabular-nums;
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 0 2px 8px;
    font-size: 13px;
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 8px 12px;
    border-radius: 8px;
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
    opacity: 0.7;
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
    margin-right: 4px;
  }
  .alarm {
    color: var(--red);
  }
  .age {
    margin-left: auto;
    font-weight: 400;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 2px;
    font-size: 12.5px;
    border-bottom: 1px solid var(--muted);
  }
  .what {
    display: inline-flex;
    align-items: center;
  }
  .empty {
    color: var(--fg-dim);
    font-size: 12.5px;
    margin: 6px 2px;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
    font: inherit;
    text-decoration: underline;
  }
</style>
