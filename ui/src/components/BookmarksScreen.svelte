<script lang="ts">
  // Every saved frequency, grouped the way the rail groups bands, over the waterfall and its panel. Typing saves, the
  // eye says whether a bookmark always has a card on the waterfall, and a frequency tunes and goes back to the waterfall.
  import type { Band, BandPlan, Bookmark, Mode } from '../lib/api';
  import { MODE_NAMES, bookmarksByBand, favourites, fmtMHz, type Saved } from '../lib/radio';
  import type { Slot } from '../lib/schedule';
  import Icon from './Icon.svelte';
  import TuneTimes from './TuneTimes.svelte';

  interface Props {
    plan: BandPlan;
    saved: Saved[];
    tuned: number;
    mode: Mode;
    slots: Slot[];
    now: number;
    onTune: (m: Saved) => void;
    onTuneAt: (hz: number, mode: Mode | null | undefined) => void;
    onBand: (b: Band) => void;
    onSave: (bandId: string, b: Bookmark) => void;
    onDelete: (bandId: string, mhz: number) => void;
    onSlots: (slots: Slot[]) => void;
    onClose: () => void;
  }
  let { plan, saved, tuned, mode, slots, now, onTune, onTuneAt, onBand, onSave, onDelete, onSlots, onClose }: Props = $props();

  let query = $state('');
  let shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? saved.filter((s) => `${s.name} ${s.notes ?? ''} ${fmtMHz(s.mhz * 1e6)} ${s.band.name}`.toLowerCase().includes(q)) : saved;
  });
  let groups = $derived(bookmarksByBand(plan, shown));
  let favs = $derived(favourites(saved));

  function save(m: Saved, change: Partial<Bookmark>) {
    const b: Bookmark = { mhz: m.mhz, name: m.name, mode: m.mode, notes: m.notes, fav: m.fav, pin: m.pin, ...change };
    if (!b.name.trim()) b.name = fmtMHz(b.mhz * 1e6);
    onSave(m.band.id, b);
  }
  const text = (e: Event) => (e.currentTarget as HTMLInputElement).value;
</script>

<section class="screen" aria-label="Bookmarks">
  <div class="list">
    <div class="head">
      <h1><Icon name="bookmark" size={18} filled /> Bookmarks</h1>
      <input class="in find" bind:value={query} placeholder="Find by name, frequency or note" aria-label="Find a bookmark" />
      <button class="close" onclick={onClose} aria-label="Back to the waterfall" title="Back to the waterfall (Esc)">×</button>
    </div>
    {#if favs.length}
      <div class="favs">
        {#each favs as f (f.band.id + f.mhz)}
          <button class="fav" class:sel={Math.abs(f.mhz * 1e6 - tuned) < 1_000} onclick={() => onTune(f)} title="Tune here and go back to the waterfall">★ {f.name} <span class="mono">{fmtMHz(f.mhz * 1e6)}</span></button>
        {/each}
      </div>
    {/if}
    {#each groups as g (g.group.id)}
      <div class="group">
        <h2>{g.group.name}</h2>
        {#each g.bands as b (b.band.id)}
          <button class="band" onclick={() => onBand(b.band)} title="Go to this band on the waterfall">{b.band.name} <span class="mono">{b.marks.length}</span></button>
          {#each b.marks as m (m.mhz)}
            <div class="row" class:sel={Math.abs(m.mhz * 1e6 - tuned) < 1_000}>
              <button class="ico" class:on={m.fav} aria-pressed={!!m.fav} onclick={() => save(m, { fav: !m.fav })} title={m.fav ? 'Take off the favourites' : 'Add to the favourites'}>{m.fav ? '★' : '☆'}</button>
              <button class="eye ico" class:on={m.pin !== false} role="switch" aria-checked={m.pin !== false} aria-label="Keep on the waterfall" onclick={() => save(m, { pin: m.pin === false })} title={m.pin === false ? 'Hidden from the waterfall unless on the air. Click to always show it.' : 'Always on the waterfall. Click to hide it unless on the air.'}><Icon name={m.pin === false ? 'eyeOff' : 'eye'} size={16} /></button>
              <button class="fq mono" onclick={() => onTune(m)} title="Tune here and go back to the waterfall">{fmtMHz(m.mhz * 1e6)}</button>
              <span class="md mono">{MODE_NAMES[m.mode ?? m.band.mode]}</span>
              <input class="in" value={m.name} aria-label="Name" onchange={(e) => save(m, { name: text(e) })} />
              <input class="in notes" value={m.notes ?? ''} placeholder="Notes" aria-label="Notes" onchange={(e) => save(m, { notes: text(e) })} />
              <button class="ico del" onclick={() => onDelete(m.band.id, m.mhz)} aria-label="Delete {m.name}" title="Delete this bookmark">×</button>
            </div>
          {/each}
        {/each}
      </div>
    {:else}
      <p class="empty">{query ? 'No bookmark matches.' : 'Nothing saved yet. Click the bookmark on any card on the waterfall to save it.'}</p>
    {/each}
  </div>
  <div class="times">
    <TuneTimes {tuned} {mode} {slots} {now} onTune={onTuneAt} {onSlots} />
  </div>
</section>

<style>
  .screen {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: flex;
    gap: 16px;
    padding: 12px 16px;
    background: var(--bg);
    overflow: hidden;
  }
  .list,
  .times {
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .list {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .times {
    flex: 0 0 320px;
  }
  @container (max-width: 800px) {
    .times {
      flex-basis: 240px;
    }
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  h1 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--fg-bright);
  }
  h1 :global(svg) {
    color: var(--hue);
  }
  .find {
    flex: 0 1 280px;
  }
  .close {
    margin-left: auto;
    width: 28px;
    height: 28px;
    border: 1px solid var(--muted);
    border-radius: 6px;
    background: transparent;
    color: var(--fg-dim);
    cursor: pointer;
  }
  .favs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .fav {
    padding: 4px 10px;
    border-radius: 6px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
    color: var(--fg-bright);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .fav.sel {
    border-color: var(--hue);
  }
  .fav .mono {
    font-size: 11px;
    color: var(--fg-dim);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  h2 {
    margin: 0 0 2px;
    padding-bottom: 3px;
    border-bottom: 1px solid var(--muted);
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--fg-dim);
  }
  .band {
    align-self: flex-start;
    margin-top: 6px;
    padding: 2px 0;
    border: 0;
    background: none;
    color: var(--fg-bright);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .band:hover {
    color: var(--hue);
  }
  .band .mono {
    font-size: 11px;
    font-weight: 400;
    color: var(--fg-dim);
  }
  .row {
    display: grid;
    grid-template-columns: 26px 26px 76px 70px minmax(120px, 1fr) minmax(120px, 2fr) 26px;
    gap: 6px;
    align-items: center;
    padding: 2px 4px;
    border-radius: 6px;
  }
  .row:hover {
    background: var(--bg-light);
  }
  .row.sel {
    background: color-mix(in srgb, var(--hue) 14%, transparent);
  }
  .ico {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--fg-dim);
    font: inherit;
    cursor: pointer;
  }
  .ico.on {
    color: var(--hue);
  }
  .ico:hover {
    background: var(--muted);
  }
  .fq {
    padding: 0;
    border: 0;
    background: none;
    color: var(--hue);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .fq:hover {
    text-decoration: underline;
  }
  .md {
    white-space: nowrap;
    font-size: 11px;
    color: var(--fg-dim);
  }
  .in {
    min-height: 28px;
    padding: 2px 8px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--fg);
    font: inherit;
    min-width: 0;
  }
  .in:hover {
    border-color: var(--muted);
  }
  .in:focus {
    border-color: var(--hue);
    background: var(--bg);
    outline: 0;
  }
  .find {
    border-color: var(--muted);
    background: var(--bg-light);
  }
  .notes {
    font-size: 12px;
    color: var(--fg-dim);
  }
  .empty {
    margin: 0;
    font-size: 13px;
    color: var(--fg-dim);
  }
</style>
