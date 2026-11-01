<script lang="ts">
  // Saved frequencies. The top block is the tuned frequency's label, and typing in it saves: no edit mode, no button.
  // Below it the starred frequencies (one click tunes) and the times to tune in. Names and notes live in the bands file
  // (bookmarks); the schedule lives in localStorage `schedule`.
  import type { Bookmark, Mode } from '../lib/api';
  import { MODE_NAMES, bookmarkNear, favourites, fmtMHz, type Saved } from '../lib/radio';
  import { DAY_LETTERS, byNext, newSlot, nextStart, when, GRACE_MS, type Slot } from '../lib/schedule';

  interface Props {
    tuned: number;
    mode: Mode;
    /** The band holding the tuned frequency; a label has to live in a band. */
    bandId: string | undefined;
    /** That band's mode: a new label records the mode only when it differs. */
    bandMode: Mode | undefined;
    saved: Saved[];
    slots: Slot[];
    now: number;
    onTune: (hz: number, mode: Mode | null | undefined) => void;
    onSave: (bandId: string, b: Bookmark) => void;
    onDelete: (bandId: string, mhz: number) => void;
    onSlots: (slots: Slot[]) => void;
  }
  let { tuned, mode, bandId, bandMode, saved, slots, now, onTune, onSave, onDelete, onSlots }: Props = $props();

  let here = $derived(bookmarkNear(saved, tuned, 1_000));
  let favs = $derived(favourites(saved));
  let nowDate = $derived(new Date(now * 1000));
  let sorted = $derived(byNext(slots, nowDate));
  const MODES = Object.keys(MODE_NAMES) as Mode[];

  /** The label as shown, with `change` applied; a new one starts from the tuned frequency. */
  function save(change: Partial<Bookmark>) {
    const base: Bookmark = here ? { mhz: here.mhz, name: here.name, mode: here.mode, notes: here.notes, fav: here.fav } : { mhz: tuned / 1e6, name: fmtMHz(tuned), mode: mode === bandMode ? null : mode };
    const b = { ...base, ...change };
    if (!b.name.trim()) b.name = fmtMHz(b.mhz * 1e6);
    if (bandId) onSave(here?.band.id ?? bandId, b);
  }

  function patch(id: string, change: Partial<Slot>) {
    onSlots(slots.map((s) => (s.id === id ? { ...s, ...change } : s)));
  }
  function toggleDay(s: Slot, d: number) {
    const days = s.days.includes(d) ? s.days.filter((x) => x !== d) : [...s.days, d].sort();
    patch(s.id, { days });
  }
  function setMhz(s: Slot, v: string) {
    const mhz = parseFloat(v);
    if (mhz >= 24 && mhz <= 1766) patch(s.id, { mhz });
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
    <h2>Favourites</h2>
    {#each favs as f (f.band.id + f.mhz)}
      <button class="item pick" class:sel={Math.abs(f.mhz * 1e6 - tuned) < 1_000} onclick={() => onTune(Math.round(f.mhz * 1e6), f.mode)}>
        <span class="name">{f.name}</span>
        <span class="mono dim">{fmtMHz(f.mhz * 1e6)} MHz · {MODE_NAMES[f.mode ?? f.band.mode]}</span>
        {#if f.notes}<span class="dim copy">{f.notes}</span>{/if}
      </button>
    {:else}
      <p class="empty">Star a frequency and it waits here, one click from tuned.</p>
    {/each}
  </div>

  <div class="col">
    <h2>Times to tune in</h2>
    {#each sorted as s (s.id)}
      {@const next = nextStart(s, new Date(nowDate.getTime() - GRACE_MS))}
      <div class="item">
        <span class="dim">{next ? when(next, nowDate) : 'Done'}</span>
        <input class="in" value={s.name} placeholder="Net or check-in name" aria-label="Name" onchange={(e) => patch(s.id, { name: text(e) })} />
        <div class="line">
          <input class="in mhz mono" value={s.mhz} inputmode="decimal" aria-label="MHz" onchange={(e) => setMhz(s, text(e))} />
          <select class="in" value={s.mode} aria-label="Mode" onchange={(e) => patch(s.id, { mode: text(e) as Mode })}>
            {#each MODES as m}<option value={m}>{MODE_NAMES[m]}</option>{/each}
          </select>
          <input class="in mono" type="time" value={s.time} aria-label="Time" onchange={(e) => text(e) && patch(s.id, { time: text(e) })} />
        </div>
        <div class="line">
          {#each DAY_LETTERS as l, d}
            <button class="day" class:on={s.days.includes(d)} aria-pressed={s.days.includes(d)} onclick={() => toggleDay(s, d)} title="Repeat every week on this day">{l}</button>
          {/each}
          {#if s.days.length === 0}<input class="in mono" type="date" value={s.date ?? ''} aria-label="Date" onchange={(e) => patch(s.id, { date: text(e) })} />{/if}
        </div>
        <div class="acts">
          <button class="quiet" onclick={() => onTune(Math.round(s.mhz * 1e6), s.mode)}>Tune now</button>
          <button class="quiet" onclick={() => onSlots(slots.filter((x) => x.id !== s.id))}>Delete</button>
        </div>
      </div>
    {:else}
      <p class="empty">Nets and check-ins you want to catch. A cue appears a few minutes before each starts, while Squelch is running.</p>
    {/each}
    <button class="quiet add" onclick={() => onSlots([...slots, newSlot(tuned / 1e6, mode, nowDate)])}>+ Add a time for {fmtMHz(tuned)} MHz</button>
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
    gap: 3px;
    font: inherit;
    cursor: pointer;
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
  .in.mhz {
    width: 84px;
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .acts {
    display: flex;
    gap: 8px;
  }
  button.quiet,
  button.star,
  button.day {
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
  button.star.on,
  button.day.on {
    color: var(--fg-bright);
    border-color: var(--pane, var(--hue));
    background: color-mix(in srgb, var(--pane, var(--hue)) 18%, transparent);
  }
  button.day {
    min-width: 28px;
    padding: 2px 0;
  }
  button.add {
    align-self: flex-start;
  }
</style>
