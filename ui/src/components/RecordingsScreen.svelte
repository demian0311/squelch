<script lang="ts">
  // Every recording saved, by day, played back in place of the live sound.
  import type { Band, Mode, Playing, SavedRecording } from '../lib/api';
  import { MODE_NAMES, fmtMHz } from '../lib/radio';
  import { byDay, clockLength, sizeLabel, timeOfDay } from '../lib/recordings';
  import Icon from './Icon.svelte';

  /** How long a deleted row offers Undo before the file goes. */
  export const UNDO_MS = 6_000;

  interface Props {
    recordings: SavedRecording[];
    playing: Playing | null;
    /** Unix seconds, ticked each second. */
    now: number;
    /** The row to highlight when the screen opens: the recording just saved. */
    focus: string | null;
    hues: Record<string, string>;
    nameOf: (hz: number, mode: Mode) => string | undefined;
    bandOf: (hz: number) => Band | undefined;
    onPlay: (file: string | null) => void;
    onPause: (paused: boolean) => void;
    onSeek: (secs: number) => void;
    onDelete: (file: string) => void;
    onLive: (hz: number, mode: Mode) => void;
    onClose: () => void;
  }
  let { recordings, playing, now, focus, hues, nameOf, bandOf, onPlay, onPause, onSeek, onDelete, onLive, onClose }: Props = $props();

  let query = $state('');
  // rows deleted but not yet gone: the file, and the timer that removes it
  let pending = $state<Record<string, number>>({});
  let copied = $state<string | null>(null);

  const nameFor = (r: SavedRecording) => nameOf(r.hz, r.mode) ?? `${fmtMHz(r.hz)} MHz`;
  const hueFor = (r: SavedRecording) => {
    const b = bandOf(r.hz);
    return b ? (hues[b.group] ?? 'var(--accent)') : 'var(--fg-dim)';
  };
  let shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return recordings;
    return recordings.filter((r) => `${nameFor(r)} ${fmtMHz(r.hz)} ${MODE_NAMES[r.mode]} ${bandOf(r.hz)?.name ?? ''} ${timeOfDay(r.at)}`.toLowerCase().includes(q));
  });
  let days = $derived(byDay(shown, now));
  let total = $derived({ n: recordings.length, secs: recordings.reduce((s, r) => s + r.secs, 0), bytes: recordings.reduce((s, r) => s + r.bytes, 0) });
  let folder = $derived(recordings[0]?.file.replace(/\/[^/]*$/, '') ?? '');

  const isPlaying = (r: SavedRecording) => playing?.file === r.file;
  function toggle(r: SavedRecording) {
    if (isPlaying(r)) onPause(!playing!.paused);
    else onPlay(r.file);
  }
  function seekTo(e: MouseEvent) {
    if (!playing) return;
    const el = e.currentTarget as HTMLElement;
    const x = (e.clientX - el.getBoundingClientRect().left) / el.clientWidth;
    onSeek(Math.max(0, Math.min(1, x)) * playing.len_secs);
  }
  function remove(r: SavedRecording) {
    if (isPlaying(r)) onPlay(null);
    pending[r.file] = window.setTimeout(() => {
      delete pending[r.file];
      onDelete(r.file);
    }, UNDO_MS);
  }
  function undo(r: SavedRecording) {
    clearTimeout(pending[r.file]);
    delete pending[r.file];
  }
  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      copied = path;
    } catch {
      copied = null;
    }
  }
  function focusRow(el: HTMLElement, file: string) {
    if (file === focus) el.scrollIntoView({ block: 'center' });
  }
</script>

<section class="screen" aria-label="Recordings">
  <div class="head">
    <h1><Icon name="tape" size={18} /> Recordings</h1>
    <input class="in find" bind:value={query} placeholder="Find a recording" title="Find by name, frequency, band or time" aria-label="Find a recording" />
    {#if total.n}
      <span class="tot mono">{total.n} · {clockLength(total.secs)} · {sizeLabel(total.bytes)}</span>
      <button class="ico wide" class:on={copied === folder} onclick={() => copyPath(folder)} aria-label="Copy the folder's path" title={copied === folder ? `Copied ${folder}` : `Copy the folder's path: ${folder}`}><Icon name="folder" size={15} /></button>
    {/if}
    <button class="close" onclick={onClose} aria-label="Back to the waterfall" title="Back to the waterfall (Esc)">×</button>
  </div>

  {#each days as d (d.label)}
    <div class="day">
      <h2>{d.label}</h2>
      {#each d.rows as r (r.file)}
        {#if pending[r.file] !== undefined}
          <div class="gone">Deleted the {timeOfDay(r.at)} recording of {nameFor(r)}. <button class="undo" onclick={() => undo(r)}>Undo</button></div>
        {:else}
          <div class="row" class:on={isPlaying(r)} class:sel={r.file === focus && !isPlaying(r)} style:--c={hueFor(r)} use:focusRow={r.file}>
            <button class="play" class:on={isPlaying(r) && !playing?.paused} onclick={() => toggle(r)} aria-label={isPlaying(r) && !playing?.paused ? 'Pause' : 'Play'} title={isPlaying(r) ? (playing?.paused ? 'Play on' : 'Pause') : 'Play this recording; the live sound pauses meanwhile'}>
              <Icon name={isPlaying(r) && !playing?.paused ? 'pause' : 'play'} size={11} />
            </button>
            <span class="t mono">{timeOfDay(r.at)}</span>
            <span class="n"><i class="dot"></i>{nameFor(r)}{#if bandOf(r.hz)}<small>{bandOf(r.hz)?.name}</small>{/if}</span>
            <span class="f mono">{fmtMHz(r.hz)}<b>{MODE_NAMES[r.mode]}</b></span>
            <span class="len mono">{clockLength(r.secs)}</span>
            <span class="acts">
              <button class="ico" class:on={copied === r.file} onclick={() => copyPath(r.file)} aria-label="Copy the file's path" title={copied === r.file ? "Copied the file's path" : "Copy the file's path"}><Icon name="folder" size={14} /></button>
              <button class="ico del" onclick={() => remove(r)} aria-label="Delete this recording" title="Delete this recording (Undo for a few seconds)"><Icon name="close" size={13} /></button>
            </span>
          </div>
          {#if isPlaying(r) && playing}
            <div class="open" style:--c={hueFor(r)}>
              <div class="scrub" role="slider" aria-label="Position" aria-valuemin={0} aria-valuemax={Math.round(playing.len_secs)} aria-valuenow={Math.round(playing.secs)} tabindex="0" onclick={seekTo} onkeydown={(e) => { if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return; e.stopPropagation(); onSeek(playing!.secs + (e.key === 'ArrowRight' ? 5 : -5)); }}>
                <i style:width="{playing.len_secs ? (100 * playing.secs) / playing.len_secs : 0}%"></i>
                <b style:left="{playing.len_secs ? (100 * playing.secs) / playing.len_secs : 0}%"></b>
              </div>
              <div class="under">
                <span class="mono">{clockLength(playing.secs)} of {clockLength(playing.len_secs)}{#if playing.done} · ended{/if}</span>
                <button class="live" onclick={() => onLive(r.hz, r.mode)} title="Stop the recording and tune the radio here">Hear it live: tune to {fmtMHz(r.hz)} MHz</button>
              </div>
            </div>
          {/if}
        {/if}
      {/each}
    </div>
  {:else}
    <p class="empty">{query ? 'No recording matches.' : 'Nothing recorded yet. Press the red record button in the top bar while listening; what you hear is saved here.'}</p>
  {/each}
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
  .in {
    min-height: 28px;
    padding: 2px 8px;
    border-radius: 6px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
    color: var(--fg);
    font: inherit;
    min-width: 0;
  }
  .in.find {
    flex: 0 1 280px;
  }
  .in:focus {
    border-color: var(--hue);
    background: var(--bg);
    outline: 0;
  }
  .tot {
    margin-left: auto;
    font-size: 11px;
    color: var(--fg-dim);
    white-space: nowrap;
  }
  .close {
    flex: none;
    width: 28px;
    height: 28px;
    border: 1px solid var(--muted);
    border-radius: 6px;
    background: transparent;
    color: var(--fg-dim);
    cursor: pointer;
  }
  .tot + .ico + .close {
    margin-left: 0;
  }
  .head .close:first-of-type {
    margin-left: auto;
  }
  .day {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  h2 {
    margin: 0 0 2px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--muted);
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--fg-dim);
  }
  .row {
    display: grid;
    grid-template-columns: 30px 48px minmax(0, 1fr) 108px 52px 60px;
    align-items: center;
    gap: 10px;
    padding: 3px 6px;
    border-radius: 6px;
  }
  .row:hover {
    background: var(--bg-light);
  }
  .row.on,
  .row.sel {
    background: color-mix(in srgb, var(--c) 16%, transparent);
  }
  .play {
    width: 24px;
    height: 24px;
    padding: 0;
    border-radius: 50%;
    border: 1px solid var(--accent);
    background: transparent;
    color: var(--accent);
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .play :global(svg) {
    fill: currentColor;
  }
  .play.on {
    background: var(--accent);
    color: var(--bg-darker);
  }
  .t {
    font-size: 12px;
    color: var(--fg-dim);
  }
  .n {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--fg-bright);
  }
  .n .dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--c);
    margin-right: 7px;
    vertical-align: 1px;
  }
  .n small {
    color: var(--fg-dim);
    margin-left: 8px;
    font-size: 11px;
  }
  .f {
    font-size: 12px;
  }
  .f b {
    font-weight: 400;
    color: var(--fg-dim);
    margin-left: 6px;
  }
  .len {
    font-size: 12px;
    text-align: right;
    color: var(--fg-dim);
  }
  .acts {
    display: flex;
    justify-content: flex-end;
    gap: 2px;
  }
  .ico {
    flex: none;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--fg-dim);
    opacity: 0.6;
    cursor: pointer;
  }
  .ico.wide {
    width: 28px;
    height: 28px;
    border: 1px solid var(--muted);
  }
  .ico:hover {
    opacity: 1;
    background: var(--muted);
  }
  .ico.on {
    opacity: 1;
    color: var(--green);
  }
  .ico.del:hover {
    color: var(--red);
  }
  .open {
    display: grid;
    gap: 6px;
    margin: 0 6px 8px 46px;
  }
  .scrub {
    height: 6px;
    border-radius: 3px;
    background: var(--bg-darker);
    position: relative;
    cursor: pointer;
  }
  .scrub:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 4px;
  }
  .scrub i {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: var(--accent);
    border-radius: 3px;
  }
  .scrub b {
    position: absolute;
    top: -4px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--fg-bright);
    transform: translateX(-50%);
  }
  .under {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    font-size: 11px;
    color: var(--fg-dim);
  }
  .live {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    font-size: 11px;
    text-decoration: underline dotted;
    cursor: pointer;
  }
  .gone {
    padding: 6px 8px;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .undo {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    text-decoration: underline dotted;
    cursor: pointer;
  }
  .empty {
    margin: 0;
    font-size: 13px;
    color: var(--fg-dim);
  }
  @container (max-width: 640px) {
    .row {
      grid-template-columns: 30px 48px minmax(0, 1fr) 52px 60px;
    }
    .f {
      display: none;
    }
  }
</style>
