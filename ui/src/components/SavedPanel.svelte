<script lang="ts">
  // A band's bookmarks. The top block is the tuned frequency's label, and typing in it saves: no edit mode, no button.
  // Beside it every bookmark in the band: a click tunes, which brings it into the top block to edit. Names and notes
  // live in the bands file.
  import type { Bookmark, Mode } from '../lib/api';
  import { MODE_NAMES, bookmarkNear, fmtMHz, type Saved } from '../lib/radio';
  import Icon from './Icon.svelte';

  interface Props {
    tuned: number;
    mode: Mode;
    /** The band holding the tuned frequency; a label has to live in a band. */
    bandId: string | undefined;
    /** That band's mode: a new label records the mode only when it differs. */
    bandMode: Mode | undefined;
    saved: Saved[];
    onTune: (hz: number, mode: Mode | null | undefined) => void;
    onSave: (bandId: string, b: Bookmark) => void;
    onDelete: (bandId: string, mhz: number) => void;
  }
  let { tuned, mode, bandId, bandMode, saved, onTune, onSave, onDelete }: Props = $props();

  let here = $derived(bookmarkNear(saved, tuned, 1_000));
  let inBand = $derived(saved.filter((s) => s.band.id === bandId));

  /** The label as shown, with `change` applied; a new one starts from the tuned frequency. */
  function save(change: Partial<Bookmark>) {
    const base: Bookmark = here ? { mhz: here.mhz, name: here.name, mode: here.mode, notes: here.notes, fav: here.fav, pin: here.pin } : { mhz: tuned / 1e6, name: fmtMHz(tuned), mode: mode === bandMode ? null : mode };
    const b = { ...base, ...change };
    if (!b.name.trim()) b.name = fmtMHz(b.mhz * 1e6);
    if (bandId) onSave(here?.band.id ?? bandId, b);
  }

  const text = (e: Event) => (e.currentTarget as HTMLInputElement).value;
</script>

<div class="pbody wrap">
  <div class="col">
    <h2>This frequency</h2>
    {#if bandId}
      {#key here?.mhz ?? tuned}
        <div class="item">
          <span class="mono big">{fmtMHz(tuned)} MHz</span>
          <input class="in" value={here?.name ?? ''} placeholder="Name it" aria-label="Name" onchange={(e) => save({ name: text(e) })} />
          <textarea class="in" rows="3" value={here?.notes ?? ''} placeholder="Notes: who uses it, when, what you heard" aria-label="Notes" onchange={(e) => save({ notes: text(e) })}></textarea>
          <div class="acts">
            <button class="star" class:on={here?.fav} aria-pressed={!!here?.fav} onclick={() => save({ fav: !here?.fav })} title={here?.fav ? 'Take off the favourites' : 'Add to the favourites'}>{here?.fav ? '★' : '☆'} Favourite</button>
            {#if here}<button class="quiet" onclick={() => onDelete(here.band.id, here.mhz)} title="Remove the name and notes from the bands file">Remove label</button>{/if}
          </div>
        </div>
      {/key}
    {:else}
      <p class="empty">No band covers {fmtMHz(tuned)} MHz, so a label has nowhere to go in the bands file.</p>
    {/if}
  </div>

  <div class="col">
    <h2>In this band</h2>
    {#each inBand as m (m.mhz)}
      <div class="item pick" class:sel={Math.abs(m.mhz * 1e6 - tuned) < 1_000}>
        <button class="tune" onclick={() => onTune(Math.round(m.mhz * 1e6), m.mode)} title="Tune here">
          <span class="name">{m.fav ? '★ ' : ''}{m.name}</span>
          <span class="mono dim">{fmtMHz(m.mhz * 1e6)} MHz · {MODE_NAMES[m.mode ?? m.band.mode]}</span>
          {#if m.notes}<span class="dim copy">{m.notes}</span>{/if}
        </button>
        <button class="eye" class:off={m.pin === false} role="switch" aria-checked={m.pin !== false} aria-label="Keep on the waterfall" title={m.pin === false ? 'Hidden from the waterfall unless on the air. Click to always show it.' : 'Always on the waterfall. Click to hide it unless on the air.'} onclick={() => onSave(m.band.id, { mhz: m.mhz, name: m.name, mode: m.mode, notes: m.notes, fav: m.fav, pin: m.pin === false })}><Icon name={m.pin === false ? 'eyeOff' : 'eye'} size={16} /></button>
      </div>
    {:else}
      <p class="empty">Nothing saved in this band. Click the bookmark on a card to save it.</p>
    {/each}
  </div>
</div>

<style>
  .pbody {
    flex: 1;
    min-height: 0;
    padding: 12px;
    overflow-y: auto;
    overscroll-behavior: contain;
    display: flex;
    flex-wrap: wrap;
    align-content: flex-start;
    gap: 12px;
  }
  .col {
    flex: 1 1 220px;
    max-width: 320px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--fg-dim);
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
    text-align: left;
    color: inherit;
  }
  .item.pick {
    flex-direction: row;
    align-items: flex-start;
    gap: 6px;
  }
  .tune {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    min-width: 0;
  }
  .eye {
    flex: none;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--pane, var(--hue));
    cursor: pointer;
  }
  .eye.off {
    color: var(--fg-dim);
  }
  .eye:hover {
    background: var(--muted);
  }
  .item.sel {
    border-color: var(--pane, var(--hue));
    background: color-mix(in srgb, var(--pane, var(--hue)) 10%, var(--bg-light));
  }
  .name {
    font-weight: 600;
    color: var(--fg-bright);
  }
  .big {
    font-size: 16px;
    font-weight: 600;
    color: var(--pane, var(--hue));
  }
  .dim,
  .empty {
    font-size: 12px;
    color: var(--fg-dim);
  }
  .empty {
    margin: 0;
    line-height: 1.45;
  }
  .in {
    min-height: 32px;
    padding: 4px 8px;
    border-radius: 6px;
    border: 1px solid var(--muted);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    min-width: 0;
  }
  textarea.in {
    resize: vertical;
  }
  .acts {
    display: flex;
    gap: 8px;
  }
  button.quiet,
  button.star {
    min-height: 28px;
    padding: 2px 10px;
    border-radius: 6px;
    border: 1px solid var(--muted);
    background: transparent;
    color: var(--fg-dim);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  button.star.on {
    color: var(--fg-bright);
    border-color: var(--pane, var(--hue));
    background: color-mix(in srgb, var(--pane, var(--hue)) 18%, transparent);
  }
</style>
