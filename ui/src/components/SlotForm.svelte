<script lang="ts">
  // One time to tune in. Under a bookmark the frequency comes from the bookmark; a time with no bookmark (`full`) has its
  // own. Everything saves as it changes.
  import { wakeSetupCommand, type Mode } from '../lib/api';
  import { MODE_NAMES } from '../lib/radio';
  import { DAY_LETTERS, EVERY_3_HOURS, EVERY_DAY, GRACE_MS, nextStart, plan333, when, withTimes, type Action, type Slot } from '../lib/schedule';

  interface Props {
    slot: Slot;
    now: number;
    full?: boolean;
    /** Why the computer can't be woken, if it can't. */
    wakeError?: string | null;
    onChange: (s: Slot) => void;
    /** Absent on a form not saved yet. */
    onDelete?: () => void;
    onTune?: () => void;
  }
  let { slot, now, full = false, wakeError = null, onChange, onDelete, onTune }: Props = $props();

  let nowDate = $derived(new Date(now * 1000));
  let next = $derived(nextStart(slot, new Date(nowDate.getTime() - GRACE_MS)));
  const MODES = Object.keys(MODE_NAMES) as Mode[];
  const ACTIONS: [Action, string, string][] = [
    ['remind', 'Remind me', 'A cue in the header and on the desktop a few minutes before'],
    ['tune', 'Tune there', 'Tunes by itself at the start'],
    ['record', 'Tune and record', 'Tunes by itself and records to a file until it ends'],
  ];
  const WEEKS: [number, string][] = [[0, 'every week'], [1, '1st of the month'], [2, '2nd of the month'], [3, '3rd of the month'], [4, '4th of the month'], [5, 'last of the month']];
  let fix = $state('');
  let copied = $state(false);
  $effect(() => {
    if (wakeError && !fix) wakeSetupCommand().then((c) => (fix = c));
  });
  async function copyFix() {
    await navigator.clipboard.writeText(fix);
    copied = true;
  }
  const patch = (change: Partial<Slot>) => onChange({ ...slot, ...change });
  const setTimes = (times: string[]) => onChange(withTimes(slot, times, nowDate));
  function toggleDay(d: number) {
    patch({ days: slot.days.includes(d) ? slot.days.filter((x) => x !== d) : [...slot.days, d].sort() });
  }
  function setMhz(v: string) {
    const mhz = parseFloat(v);
    if (mhz >= 24 && mhz <= 1766) patch({ mhz });
  }
  function addTime(e: Event) {
    const el = e.currentTarget as HTMLInputElement;
    if (el.value) setTimes([...slot.times, el.value]);
    el.value = '';
  }
  const text = (e: Event) => (e.currentTarget as HTMLInputElement).value;
</script>

<div class="form">
  <div class="line">
    <input class="in grow" value={slot.name} placeholder="Net or check-in name" aria-label="Name" onchange={(e) => patch({ name: text(e) })} />
    {#if full}
      <input class="in mhz mono" value={slot.mhz} inputmode="decimal" aria-label="MHz" onchange={(e) => setMhz(text(e))} />
      <select class="in" value={slot.mode} aria-label="Mode" onchange={(e) => patch({ mode: text(e) as Mode })}>
        {#each MODES as m}<option value={m}>{MODE_NAMES[m]}</option>{/each}
      </select>
    {/if}
  </div>
  <div class="line">
    {#each slot.times as t (t)}
      <button class="chip on mono" disabled={slot.times.length === 1 && !!onDelete} onclick={() => setTimes(slot.times.filter((x) => x !== t))} title="Take this time off">{t}{#if slot.times.length > 1 || !onDelete} ×{/if}</button>
    {/each}
    <input class="in mono" type="time" aria-label="Add a time" title="Add a time" onchange={addTime} />
    <button class="chip" onclick={() => onChange(withTimes({ ...slot, days: slot.days.length ? slot.days : EVERY_DAY }, EVERY_3_HOURS, nowDate))}>Every 3 hours</button>
    <button class="chip" onclick={() => onChange(plan333(slot))} title="Every 3 hours, for 3 minutes, tuning by itself">3-3-3 plan</button>
  </div>
  <div class="line">
    <span class="days">
      {#each DAY_LETTERS as l, d}
        <button class="chip day" class:on={slot.days.includes(d)} aria-pressed={slot.days.includes(d)} onclick={() => toggleDay(d)} title="Repeat every week on this day">{l}</button>
      {/each}
    </span>
    <button class="chip" onclick={() => patch({ days: EVERY_DAY })}>Every day</button>
    {#if slot.days.length}
      <select class="in" value={slot.week ?? 0} aria-label="Which weeks" onchange={(e) => patch({ week: +text(e) || null })}>
        {#each WEEKS as [w, label]}<option value={w}>{label}</option>{/each}
      </select>
    {:else}
      <input class="in mono" type="date" value={slot.date ?? ''} aria-label="Date" onchange={(e) => patch({ date: text(e) })} />
    {/if}
  </div>
  <div class="line">
    <label class="dim for">For <input class="in mins mono" type="number" min="1" max="1440" value={slot.minutes} aria-label="Minutes" onchange={(e) => +text(e) >= 1 && patch({ minutes: Math.round(+text(e)) })} /> min</label>
    {#each ACTIONS as [a, label, why]}
      <button class="chip" class:on={slot.action === a} aria-pressed={slot.action === a} onclick={() => patch({ action: a })} title={why}>{label}</button>
    {/each}
    {#if slot.action !== 'remind'}
      <button class="chip" class:on={slot.go_back} role="switch" aria-checked={slot.go_back} onclick={() => patch({ go_back: !slot.go_back })} title="When it ends, go back to the frequency it took over from">Go back after</button>
    {/if}
  </div>
  <div class="line">
    <span class="dim">{slot.times.length ? (next ? `Next: ${when(next, nowDate)}` : 'Done') : 'Add a time to save this'}</span>
    <span class="acts">
      {#if onTune}<button class="chip" onclick={onTune}>Tune now</button>{/if}
      {#if onDelete}<button class="chip" onclick={onDelete}>Delete</button>{/if}
    </span>
  </div>
  {#if slot.action !== 'remind' && slot.times.length}
    {#if wakeError}
      <p class="dim wake">Runs only while the computer is awake. To let Squelch wake it, run this once in a terminal:</p>
      <div class="line fix">
        <code class="copy">{fix}</code>
        <button class="chip" onclick={copyFix}>{copied ? 'Copied' : 'Copy'}</button>
      </div>
    {:else}
      <p class="dim wake">Wakes the computer if it is asleep. Squelch has to be open.</p>
    {/if}
  {/if}
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 8px;
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
  .in.mins {
    width: 64px;
  }
  .for {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dim {
    font-size: 12px;
    color: var(--fg-dim);
  }
  .wake {
    margin: 0;
  }
  .fix code {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    font-size: 12px;
  }
  .acts {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }
  .chip {
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
  .chip.day {
    min-width: 26px;
    padding: 2px 0;
  }
  .chip.on {
    color: var(--fg-bright);
    border-color: var(--c);
    background: color-mix(in srgb, var(--c) 22%, transparent);
  }
  .chip:disabled {
    cursor: default;
  }
</style>
