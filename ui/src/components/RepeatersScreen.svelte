<script lang="ts">
  // The Repeaters screen: who is talking near you, FM or DMR, and one click to listen. No sentence says
  // what goes on: the ruler shows where the capture sits, the row's place says whether it is heard, the
  // glyph and the level bar say what it carries, and the open row holds the squelch mark and the facts.
  import MapSplit from './MapSplit.svelte';
  import type { Snippet } from 'svelte';
  import { fmtMHz } from '../lib/radio';
  import { shortAgo, type RepRow } from '../lib/repscreen';
  import Icon from './Icon.svelte';

  interface Props {
    rows: RepRow[];
    /** The band's edges, Hz. */
    band: { lo: number; hi: number };
    /** Where the capture sits, Hz. */
    capture: { centre: number; half: number };
    /** The squelch, dB above the floor, 0 = never muted; `meterDb` is the bar's full scale. */
    squelch: number;
    meterDb: number;
    onSquelch: (db: number) => void;
    onRow: (hz: number) => void;
    onSeeRadio: () => void;
    /** the rows' share of the width, App's own panel share: the map is the waterfall's place */
    share: number;
    onShare: (share: number, save: boolean) => void;
    map: Snippet;
  }
  let { rows, band, capture, squelch, meterDb, onSquelch, onRow, onSeeRadio, map, share, onShare }: Props = $props();

  const STATE_ICON = { playing: 'speaker', talking: 'zap', under: 'mute', quiet: 'quiet', away: 'arrowRight' } as const;
  let heard = $derived(rows.filter((r) => r.p.reach && r.heard));
  let away = $derived(rows.filter((r) => r.p.reach && !r.heard));
  let far = $derived(rows.filter((r) => !r.p.reach));
  /** The row opened by its chevron; the playing and held rows are open by themselves. */
  let openId = $state<string | null>(null);
  let farOpen = $state(false);
  const isOpen = (r: RepRow) => r.state === 'playing' || r.held || r.p.id === openId;
  const pct = (hz: number) => ((hz - band.lo) / (band.hi - band.lo)) * 100;
  const GLOSS = {
    dmr: 'DMR: digital voice. The repeater is silent between calls, so a quiet row is normal; a call fills it and plays. Two conversations fit at once, one per slot.',
    fm: 'Plain FM voice. A carrier on the frequency means someone is keyed up.',
    far: 'Probably too far for this band’s usual range, from the distance alone; a repeater on a mountain or a tall tower may still come in.',
    ruler: 'The band. The lit span is what the radio hears at once; a tick is a repeater, bright while it is on air. Click a tick to go there.',
    mark: 'The bar is the signal. Drag the mark: the sound stays off while the signal is left of it (radio calls this the squelch). Far left never mutes.',
    slots: 'DMR carries two conversations at once, one per slot: green while a call fills it, orange when that call is encrypted.',
  };
  const rowTitle = (r: RepRow) => `${r.say} · ${r.detail}. ${r.held ? 'Held: only this one plays. Click to let go.' : r.heard ? 'Click to stay on this repeater.' : 'Click to move the radio onto this repeater.'}`;
  const level = (r: RepRow) => (r.snr === null ? 0 : Math.max(0, Math.min(100, (r.snr / meterDb) * 100)));

  function dragMark(e: PointerEvent) {
    if (e.button !== 0) return;
    e.stopPropagation();
    const track = e.currentTarget as HTMLElement;
    track.setPointerCapture(e.pointerId);
    const box = track.getBoundingClientRect();
    const set = (m: PointerEvent) => {
      const db = Math.round(((m.clientX - box.left) / box.width) * meterDb);
      onSquelch(db < 1 ? 0 : Math.min(meterDb, db));
    };
    set(e);
    const stop = () => {
      track.removeEventListener('pointermove', set);
      track.removeEventListener('pointerup', stop);
    };
    track.addEventListener('pointermove', set);
    track.addEventListener('pointerup', stop);
  }
  function markKey(e: KeyboardEvent) {
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') onSquelch(Math.min(meterDb, squelch + 1));
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') onSquelch(Math.max(0, squelch - 1));
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

{#snippet rowsCell()}
    <div class="list">
      <div class="ruler" title={GLOSS.ruler}>
        <span class="end">{fmtMHz(band.lo, 1)}</span><span class="end r">{fmtMHz(band.hi, 1)}</span>
        <div class="band"></div>
        <div class="cap" style:left="{pct(capture.centre - capture.half)}%" style:width="{pct(capture.centre + capture.half) - pct(capture.centre - capture.half)}%"></div>
        {#each rows as r (r.p.id)}
          <button class="tk" class:on={r.state === 'playing' || r.state === 'talking'} class:far={!r.p.reach} style:left="{pct(r.p.hz)}%" aria-label={r.p.call} title="{r.p.call} · {fmtMHz(r.p.hz, 4)}" onclick={() => onRow(r.p.hz)}></button>
        {/each}
      </div>

      <div class="rows" role="list">
        {#snippet row(r: RepRow)}
          {@const open = isOpen(r)}
          <div class="item {r.state}" class:held={r.held} class:far={!r.p.reach} class:open role="listitem">
            <button class="row" onclick={() => onRow(r.p.hz)} title={rowTitle(r)}>
              <span class="who"><b>{r.p.call}</b><span class="city">{r.p.city}</span></span>
              <span class="num">{r.p.km < 10 ? r.p.km.toFixed(1) : Math.round(r.p.km)} km</span>
              <span class="num">{fmtMHz(r.p.hz, 4)}</span>
              <span class="chip" class:isdmr={r.dmr} title={r.dmr ? GLOSS.dmr : GLOSS.fm}>{r.dmr ? 'DMR' : 'FM'}</span>
              <span class="gl {r.held ? 'held' : r.state}"><Icon name={r.held ? 'lock' : STATE_ICON[r.state]} size={16} /></span>
              <span class="sig">
                <span class="lv" style:--sq="{(squelch / meterDb) * 100}%"><i class:under={r.state === 'under'} style:width="{level(r)}%"></i>{#if squelch > 0}<b></b>{/if}</span>
                {#if r.dmr}<span class="slots" title={GLOSS.slots}>{#each r.facts.slots.length ? r.facts.slots : [{ call: false, encrypted: false }, { call: false, encrypted: false }] as s, i (i)}<s class:on={s.call && !s.encrypted} class:enc={s.encrypted}></s>{/each}</span>{/if}
              </span>
            </button>
            {#if !(r.state === 'playing' || r.held)}
              <button class="chev" aria-label={open ? 'Close' : 'Open'} aria-expanded={open} onclick={() => (openId = open ? null : r.p.id)}><Icon name={open ? 'chevronDown' : 'chevronRight'} size={16} /></button>
            {/if}
            {#if open}
              <div class="more">
                <div class="big" role="slider" tabindex="0" aria-label="Mute line (squelch)" aria-valuemin={0} aria-valuemax={meterDb} aria-valuenow={squelch} aria-valuetext={squelch > 0 ? `${squelch} dB` : 'Off'} title={GLOSS.mark} onpointerdown={dragMark} onkeydown={markKey}>
                  <i class:under={r.state === 'under'} style:width="{level(r)}%"></i>
                  <b class:off={squelch === 0} style:left="{(squelch / meterDb) * 100}%"></b>
                </div>
                <div class="scale mono"><span>floor</span><span>{r.snr === null ? '' : `${r.snr.toFixed(0)} dB`}</span><span>+{meterDb} dB</span></div>
                <div class="facts">
                  {#if r.facts.to}<span title="Who the call is for, and from"><Icon name="users" size={13} />{r.facts.to}</span>{/if}
                  {#if r.facts.length}<span title="How long this call has run"><Icon name="zap" size={13} /><span class="mono">{r.facts.length}</span></span>{/if}
                  {#if r.facts.lastSecs !== null && !r.facts.length}<span title="Since this repeater last carried something"><Icon name="clock" size={13} /><span class="mono">{shortAgo(r.facts.lastSecs)}</span></span>{/if}
                </div>
              </div>
            {/if}
          </div>
        {/snippet}
        {#each heard as r (r.p.id)}{@render row(r)}{/each}
        {#if heard.length && away.length}<div class="hr"></div>{/if}
        {#each away as r (r.p.id)}{@render row(r)}{/each}
        {#if far.length}
          <button class="fold" class:open={farOpen} aria-expanded={farOpen} title={GLOSS.far} onclick={() => (farOpen = !farOpen)}><Icon name={farOpen ? 'chevronDown' : 'chevronRight'} size={14} /><span class="mono">{far.length}</span> too far for this band</button>
          {#if farOpen}{#each far as r (r.p.id)}{@render row(r)}{/each}{/if}
        {/if}
        {#if !rows.length}
          <p class="empty">No repeater this radio can play is listed near you on this band.</p>
        {/if}
      </div>
    </div>
{/snippet}
{#snippet mapCell()}
    <div class="map">{@render map()}</div>
{/snippet}

<div class="rep">
  <MapSplit {share} {onShare} map={mapCell} rows={rowsCell} />

  <p class="foot">
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
  .list {
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  /* the ruler: the band, the capture's span, a tick per repeater */
  .ruler {
    position: relative;
    height: 24px;
    margin: 0 2px;
    flex: none;
  }
  .ruler .end {
    position: absolute;
    top: 0;
    font-family: var(--mono);
    font-size: 10px;
    color: var(--fg-dim);
  }
  .ruler .end.r {
    right: 0;
  }
  .ruler .band {
    position: absolute;
    left: 0;
    right: 0;
    top: 14px;
    height: 2px;
    background: var(--muted);
  }
  .ruler .cap {
    position: absolute;
    top: 9px;
    height: 12px;
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    border: 1px solid var(--accent);
    border-radius: 3px;
  }
  .ruler .tk {
    position: absolute;
    top: 11px;
    width: 2px;
    height: 8px;
    margin-left: -1px;
    padding: 0;
    border: 0;
    border-radius: 1px;
    background: var(--fg-dim);
    cursor: pointer;
  }
  .ruler .tk.on {
    top: 9px;
    height: 12px;
    background: var(--green);
    box-shadow: 0 0 6px var(--green);
  }
  .ruler .tk.far {
    opacity: 0.35;
  }
  .rows {
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .item {
    position: relative;
    border-radius: var(--r);
    border: 1px solid var(--muted);
    background: var(--bg-light);
  }
  .item:hover {
    border-color: var(--fg-dim);
  }
  .item.talking,
  .item.playing {
    background: color-mix(in srgb, var(--green) 12%, var(--bg-light));
  }
  .item.playing {
    border-color: var(--green);
  }
  .item.held {
    border-color: var(--accent);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .item.away {
    opacity: 0.55;
  }
  .item.far {
    opacity: 0.45;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(96px, 1.2fr) 48px 80px 40px 20px minmax(80px, 1.4fr);
    gap: 8px;
    align-items: center;
    width: 100%;
    padding: 7px 28px 7px 10px;
    border: 0;
    background: none;
    color: var(--fg);
    text-align: left;
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .chev {
    position: absolute;
    top: 7px;
    right: 6px;
    display: inline-flex;
    padding: 2px;
    border: 0;
    background: none;
    color: var(--fg-dim);
    cursor: pointer;
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
  .num,
  .mono {
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
  .gl {
    display: inline-flex;
    color: var(--fg-dim);
  }
  .gl.playing {
    color: var(--green);
  }
  .gl.talking {
    color: var(--yellow);
  }
  .gl.under {
    color: var(--orange);
  }
  .gl.held {
    color: var(--accent);
  }
  .sig {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  /* the level bar: the waterfall's ramp up to the signal, the squelch as a mark across it */
  .lv {
    position: relative;
    flex: 1;
    height: 6px;
    border-radius: 3px;
    background: var(--muted);
  }
  .lv i,
  .big i {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 3px;
    background: var(--ramp, linear-gradient(90deg, var(--green), var(--yellow)));
  }
  .lv i.under,
  .big i.under {
    background: var(--fg-dim);
  }
  .lv b {
    position: absolute;
    top: -4px;
    bottom: -4px;
    left: var(--sq);
    width: 2px;
    margin-left: -1px;
    background: var(--fg);
    border-radius: 1px;
  }
  .slots {
    display: inline-flex;
    gap: 3px;
  }
  .slots s {
    display: block;
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--muted);
    text-decoration: none;
  }
  .slots s.on {
    background: var(--green);
  }
  .slots s.enc {
    background: var(--orange);
  }
  .more {
    padding: 4px 10px 8px;
    border-top: 1px dashed var(--muted);
  }
  .big {
    position: relative;
    height: 12px;
    margin: 12px 0 4px;
    border-radius: 3px;
    background: var(--muted);
    cursor: pointer;
    touch-action: none;
  }
  .big:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 4px;
  }
  .big b {
    position: absolute;
    top: -6px;
    bottom: -6px;
    width: 3px;
    margin-left: -1px;
    background: var(--fg);
    border-radius: 1px;
    cursor: ew-resize;
  }
  .big b::after {
    content: '';
    position: absolute;
    left: -5px;
    bottom: -5px;
    width: 13px;
    height: 5px;
    background: var(--fg);
    border-radius: 0 0 3px 3px;
  }
  .big b.off {
    opacity: 0.4;
  }
  .scale {
    display: flex;
    justify-content: space-between;
    font-size: 10px;
  }
  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
    align-items: center;
    margin-top: 6px;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .facts > span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .facts .mono {
    color: var(--fg);
  }
  .hr {
    flex: none;
    height: 1px;
    margin: 4px 2px;
    background: var(--muted);
  }
  .fold {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    padding: 5px 10px;
    border: 0;
    background: none;
    color: var(--fg-dim);
    font: inherit;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
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
    justify-content: flex-end;
    margin: 0;
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
      grid-template-columns: minmax(90px, 1fr) 20px minmax(60px, 1.2fr);
    }
    .row .num,
    .row .chip {
      display: none;
    }
  }
</style>
