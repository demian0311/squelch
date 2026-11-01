<script lang="ts">
  // Times to tune in, kept in localStorage `schedule` by App; a reminder shows in the header before each start.
  import type { Mode } from '../lib/api';
  import { MODE_NAMES, fmtMHz } from '../lib/radio';
  import { DAY_LETTERS, byNext, newSlot, nextStart, when, GRACE_MS, type Slot } from '../lib/schedule';

  interface Props {
    tuned: number;
    mode: Mode;
    slots: Slot[];
    now: number;
    onTune: (hz: number, mode: Mode | null | undefined) => void;
    onSlots: (slots: Slot[]) => void;
  }
  let { tuned, mode, slots, now, onTune, onSlots }: Props = $props();

  let nowDate = $derived(new Date(now * 1000));
  let sorted = $derived(byNext(slots, nowDate));
  const MODES = Object.keys(MODE_NAMES) as Mode[];

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

<style>
  .col {
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
  button.day.on {
    color: var(--fg-bright);
    border-color: var(--hue);
    background: color-mix(in srgb, var(--hue) 18%, transparent);
  }
  button.day {
    min-width: 28px;
    padding: 2px 0;
  }
  button.add {
    align-self: flex-start;
  }
</style>
