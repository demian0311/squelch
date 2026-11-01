<script lang="ts">
  // One time to tune in. Under a bookmark only the time and days show, the rest comes from the bookmark; a time with no
  // bookmark (`full`) also has its own name, frequency and mode.
  import type { Mode } from '../lib/api';
  import { MODE_NAMES } from '../lib/radio';
  import { DAY_LETTERS, GRACE_MS, nextStart, when, withTime, type Slot } from '../lib/schedule';

  interface Props {
    slot: Slot;
    now: number;
    full?: boolean;
    onChange: (s: Slot) => void;
    /** Absent on a form not saved yet. */
    onDelete?: () => void;
    onTune?: () => void;
  }
  let { slot, now, full = false, onChange, onDelete, onTune }: Props = $props();

  let nowDate = $derived(new Date(now * 1000));
  let next = $derived(nextStart(slot, new Date(nowDate.getTime() - GRACE_MS)));
  const MODES = Object.keys(MODE_NAMES) as Mode[];
  const patch = (change: Partial<Slot>) => onChange({ ...slot, ...change });
  function toggleDay(d: number) {
    patch({ days: slot.days.includes(d) ? slot.days.filter((x) => x !== d) : [...slot.days, d].sort() });
  }
  function setMhz(v: string) {
    const mhz = parseFloat(v);
    if (mhz >= 24 && mhz <= 1766) patch({ mhz });
  }
  const text = (e: Event) => (e.currentTarget as HTMLInputElement).value;
</script>

<div class="form">
  {#if full}
    <div class="line">
      <input class="in grow" value={slot.name} placeholder="Net or check-in name" aria-label="Name" onchange={(e) => patch({ name: text(e) })} />
      <input class="in mhz mono" value={slot.mhz} inputmode="decimal" aria-label="MHz" onchange={(e) => setMhz(text(e))} />
      <select class="in" value={slot.mode} aria-label="Mode" onchange={(e) => patch({ mode: text(e) as Mode })}>
        {#each MODES as m}<option value={m}>{MODE_NAMES[m]}</option>{/each}
      </select>
    </div>
  {/if}
  <div class="line">
    <input class="in mono" type="time" value={slot.time} aria-label="Time" onchange={(e) => text(e) && onChange(withTime(slot, text(e), nowDate))} />
    <span class="days">
      {#each DAY_LETTERS as l, d}
        <button class="day" class:on={slot.days.includes(d)} aria-pressed={slot.days.includes(d)} onclick={() => toggleDay(d)} title="Repeat every week on this day">{l}</button>
      {/each}
    </span>
    {#if slot.days.length === 0}<input class="in mono" type="date" value={slot.date ?? ''} aria-label="Date" onchange={(e) => patch({ date: text(e) })} />{/if}
  </div>
  <div class="line">
    <span class="dim">{slot.time ? (next ? `Next: ${when(next, nowDate)}` : 'Done') : 'Set a time to save this'}</span>
    <span class="acts">
      {#if onTune}<button class="quiet" onclick={onTune}>Tune now</button>{/if}
      {#if onDelete}<button class="quiet" onclick={onDelete}>Delete</button>{/if}
    </span>
  </div>
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .days {
    display: flex;
    gap: 3px;
  }
  .grow {
    flex: 1 1 140px;
  }
  .in {
    min-height: 30px;
    padding: 2px 8px;
    border-radius: 6px;
    border: 1px solid var(--muted);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    min-width: 0;
  }
  .in.mhz {
    width: 84px;
  }
  .dim {
    font-size: 12px;
    color: var(--fg-dim);
  }
  .acts {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }
  button.quiet,
  button.day {
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
  button.day {
    min-width: 26px;
    padding: 2px 0;
  }
  button.day.on {
    color: var(--fg-bright);
    border-color: var(--c);
    background: color-mix(in srgb, var(--c) 22%, transparent);
  }
</style>
