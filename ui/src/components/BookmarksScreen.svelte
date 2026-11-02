<script lang="ts">
  // Every saved frequency, grouped the way the rail groups bands and in the rail's colours, over the waterfall and its
  // panel. Typing saves, and a frequency tunes and goes back to the waterfall. A lit icon says a row has that thing: the
  // note and the clock open under the row. A time belongs to a bookmark by frequency.
  import type { Band, BandPlan, Bookmark } from '../lib/api';
  import { MODE_NAMES, bandShort, bookmarksByBand, fmtMHz, type Saved } from '../lib/radio';
  import { GRACE_MS, byNext, loneSlots, newSlot, nextStart, slotsAt, type Slot } from '../lib/schedule';
  import Icon from './Icon.svelte';
  import SlotForm from './SlotForm.svelte';

  interface Props {
    /** The bands the rail shows. */
    plan: BandPlan;
    saved: Saved[];
    hues: Record<string, string>;
    tuned: number;
    slots: Slot[];
    now: number;
    onTune: (m: Saved) => void;
    onTuneAt: (s: Slot) => void;
    onBand: (b: Band) => void;
    onSave: (bandId: string, b: Bookmark) => void;
    onDelete: (bandId: string, mhz: number) => void;
    onSlots: (slots: Slot[]) => void;
    onClose: () => void;
    /** Why the computer can't be woken, if it can't. */
    wakeError: string | null;
  }
  let { plan, saved, hues, tuned, slots, now, wakeError, onTune, onTuneAt, onBand, onSave, onDelete, onSlots, onClose }: Props = $props();

  let query = $state('');
  let inRail = $derived(saved.filter((s) => plan.bands.some((b) => b.id === s.band.id)));
  let shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? inRail.filter((s) => `${s.name} ${s.notes ?? ''} ${fmtMHz(s.mhz * 1e6)} ${s.band.name}`.toLowerCase().includes(q)) : inRail;
  });
  let groups = $derived(bookmarksByBand(plan, shown));
  let nowDate = $derived(new Date(now * 1000));
  let lone = $derived(byNext(loneSlots(slots, saved), nowDate));
  const coming = (s: Slot) => !!nextStart(s, new Date(nowDate.getTime() - GRACE_MS));

  // what is open under a row, one at a time
  let open = $state<{ key: string; what: 'note' | 'clock' } | null>(null);
  // a time being filled in; it joins the schedule once it has a time of day
  let draft = $state<Slot | null>(null);
  const keyOf = (m: Saved) => `${m.band.id}|${m.mhz}`;
  const isOpen = (m: Saved, what: 'note' | 'clock') => open?.key === keyOf(m) && open.what === what;

  function blank(m: Saved): Slot {
    return newSlot(m.name, m.mhz, m.mode ?? m.band.mode, nowDate);
  }
  function toggle(m: Saved, what: 'note' | 'clock') {
    open = isOpen(m, what) ? null : { key: keyOf(m), what };
    draft = open && what === 'clock' && !slotsAt(slots, m.mhz).length ? blank(m) : null;
  }
  function change(next: Slot) {
    if (draft?.id !== next.id) return onSlots(slots.map((s) => (s.id === next.id ? next : s)));
    if (next.times.length) {
      onSlots([...slots, next]);
      draft = null;
    } else draft = next;
  }

  function save(m: Saved, change: Partial<Bookmark>) {
    const b: Bookmark = { mhz: m.mhz, name: m.name, mode: m.mode, notes: m.notes, ...change };
    if (!b.name.trim()) b.name = fmtMHz(b.mhz * 1e6);
    onSave(m.band.id, b);
    // a time still called after the bookmark stays called after it
    if (b.name !== m.name) onSlots(slots.map((s) => (s.name === m.name && slotsAt([s], m.mhz).length ? { ...s, name: b.name } : s)));
  }
  const text = (e: Event) => (e.currentTarget as HTMLInputElement).value;
  const focus = (el: HTMLElement) => el.focus();
</script>

<section class="screen" aria-label="Bookmarks">
  <div class="head">
    <h1><Icon name="bookmark" size={18} filled /> Bookmarks</h1>
    <input class="in find" bind:value={query} placeholder="Find a bookmark" title="Find by name, frequency or note" aria-label="Find a bookmark" />
    <button class="close" onclick={onClose} aria-label="Back to the waterfall" title="Back to the waterfall (Esc)">×</button>
  </div>


  {#each groups as g (g.group.id)}
    <div class="group" style:--c={hues[g.group.id]}>
      <h2><span class="swatch"></span>{g.group.name}</h2>
      {#each g.bands as b (b.band.id)}
        <button class="band" onclick={() => onBand(b.band)} title="Go to this band on the waterfall">
          <span class="badge" class:txt={bandShort(b.band)}>{#if bandShort(b.band)}{bandShort(b.band)}{:else}<Icon name={b.band.icon} size={16} />{/if}</span>
          {b.band.name} <span class="mono count">{b.marks.length}</span>
        </button>
        {#each b.marks as m (m.mhz)}
          {@const times = slotsAt(slots, m.mhz)}
          <div class="row" class:sel={Math.abs(m.mhz * 1e6 - tuned) < 1_000}>
            <button class="fq mono" onclick={() => onTune(m)} title="Tune here and go back to the waterfall">{fmtMHz(m.mhz * 1e6)}</button>
            <span class="md mono">{MODE_NAMES[m.mode ?? m.band.mode]}</span>
            <input class="in name" value={m.name} aria-label="Name" onchange={(e) => save(m, { name: text(e) })} />
            <button class="ico" class:on={!!m.notes} class:open={isOpen(m, 'note')} aria-expanded={isOpen(m, 'note')} aria-label="Notes" onclick={() => toggle(m, 'note')} title={m.notes || 'Add a note'}><Icon name="note" size={16} /></button>
            <button class="ico" class:on={times.some(coming)} class:open={isOpen(m, 'clock')} aria-expanded={isOpen(m, 'clock')} aria-label="Times to tune in" onclick={() => toggle(m, 'clock')} title="Times to tune in: a cue appears a few minutes before each, while Squelch is running"><Icon name="clock" size={16} /></button>
            <button class="ico del" onclick={() => onDelete(m.band.id, m.mhz)} aria-label="Delete {m.name}" title="Delete this bookmark"><Icon name="close" size={14} /></button>
          </div>
          {#if isOpen(m, 'note')}
            <div class="more">
              <textarea class="in note" rows="3" value={m.notes ?? ''} placeholder="Who uses it, when, what you heard" aria-label="Notes" use:focus onchange={(e) => save(m, { notes: text(e) })}></textarea>
            </div>
          {:else if isOpen(m, 'clock')}
            <div class="more">
              {#each draft ? [...times, draft] : times as s (s.id)}
                <SlotForm slot={s} {now} {wakeError} onChange={change} onDelete={s.id === draft?.id ? undefined : () => onSlots(slots.filter((x) => x.id !== s.id))} />
              {/each}
              {#if !draft}<button class="add" onclick={() => (draft = blank(m))}>+ Add another time</button>{/if}
            </div>
          {/if}
        {/each}
      {/each}
    </div>
  {:else}
    <p class="empty">{query ? 'No bookmark matches.' : 'Nothing saved yet. Tune a frequency and click the bookmark beside its name in the top bar.'}</p>
  {/each}

  {#if lone.length}
    <div class="group" style:--c="var(--hue)">
      <h2><Icon name="clock" size={13} /> Times with no bookmark</h2>
      {#each lone as s (s.id)}
        <div class="more"><SlotForm slot={s} {now} {wakeError} full onChange={change} onDelete={() => onSlots(slots.filter((x) => x.id !== s.id))} onTune={() => onTuneAt(s)} /></div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .screen {
    position: absolute;
    inset: 0;
    z-index: 20;
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 12px 16px;
    background: var(--bg);
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .screen > :global(*) {
    flex: none;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  h1 {
    flex: none;
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
  .in.find {
    flex: 0 1 280px;
    border-color: var(--muted);
    background: var(--bg-light);
  }
  .close {
    flex: none;
    margin-left: auto;
    width: 28px;
    height: 28px;
    border: 1px solid var(--muted);
    border-radius: 6px;
    background: transparent;
    color: var(--fg-dim);
    cursor: pointer;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 2px;
    padding-bottom: 4px;
    border-bottom: 1px solid color-mix(in srgb, var(--c) 45%, transparent);
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--c);
  }
  .swatch {
    width: 7px;
    height: 7px;
    border-radius: 2px;
    background: var(--c);
  }
  .band {
    align-self: flex-start;
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    padding: 2px 0;
    border: 0;
    background: none;
    color: var(--fg-bright);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .band:hover {
    color: var(--c);
  }
  .badge {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 7px;
    background: color-mix(in srgb, var(--c) 16%, var(--bg-dark));
    color: var(--c);
  }
  .badge.txt {
    font-family: var(--mono);
    font-size: 10px;
  }
  .count {
    font-size: 11px;
    font-weight: 400;
    color: var(--fg-dim);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 4px;
    border-radius: 6px;
  }
  .row:hover {
    background: var(--bg-light);
  }
  .row.sel {
    background: color-mix(in srgb, var(--c) 16%, transparent);
  }
  .ico {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--fg-dim);
    opacity: 0.55;
    cursor: pointer;
  }
  .ico:hover {
    opacity: 1;
    background: var(--muted);
  }
  /* lit: this row has one */
  .ico.on {
    color: var(--c);
    opacity: 1;
  }
  .ico.open {
    opacity: 1;
    background: color-mix(in srgb, var(--c) 28%, transparent);
  }
  .fq {
    flex: none;
    width: 68px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--c);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .fq:hover {
    text-decoration: underline;
  }
  .md {
    flex: none;
    width: 74px;
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
    border-color: var(--c, var(--hue));
    background: var(--bg);
    outline: 0;
  }
  .name {
    flex: 1 1 100px;
  }
  .note {
    width: 100%;
    box-sizing: border-box;
    border-color: var(--muted);
    background: var(--bg);
    resize: vertical;
  }
  .more {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 2px 4px 6px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid color-mix(in srgb, var(--c) 40%, var(--muted));
    background: var(--bg-light);
  }
  .add {
    align-self: flex-start;
    min-height: 26px;
    padding: 2px 10px;
    border-radius: 6px;
    border: 1px solid var(--muted);
    background: transparent;
    color: var(--fg-dim);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .empty {
    margin: 0;
    font-size: 13px;
    color: var(--fg-dim);
  }
  /* the mode goes first, then the name takes a line of its own */
  @container (max-width: 560px) {
    .md {
      display: none;
    }
  }
  @container (max-width: 340px) {
    .row {
      flex-wrap: wrap;
    }
    .fq {
      flex: 1;
    }
    .name {
      order: 9;
      flex-basis: 100%;
    }
  }
</style>
