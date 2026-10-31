<script lang="ts">
  import type { Snippet } from 'svelte';

  // A panel under the waterfall that folds to one line. Open or closed is remembered per `id`,
  // and so is its height once someone drags its top edge; the waterfall takes the rest.
  // `fixed`: the open tray always takes its full share and its content never scrolls; children scroll themselves.
  let {
    id,
    title,
    summary = '',
    accent = 'var(--accent)',
    fixed = false,
    children,
  }: { id: string; title: string; summary?: string; accent?: string; fixed?: boolean; children: Snippet } = $props();

  let open = $state(read());
  // % of the column; null keeps the default share
  let share = $state<number | null>(readShare());
  let section: HTMLElement;

  const MIN_SHARE = 15;
  const MAX_SHARE = 80;
  const KEY_STEP = 5;

  function readShare(): number | null {
    try {
      const v = Number(localStorage.getItem(`tray.${id}.share`));
      return v >= MIN_SHARE && v <= MAX_SHARE ? v : null;
    } catch {
      return null;
    }
  }
  function setShare(v: number | null) {
    share = v === null ? null : Math.round(Math.min(MAX_SHARE, Math.max(MIN_SHARE, v)) * 10) / 10;
    try {
      if (share === null) localStorage.removeItem(`tray.${id}.share`);
      else localStorage.setItem(`tray.${id}.share`, String(share));
    } catch {
      // remembering the size is a nicety
    }
  }
  // % heights resolve against the parent's content box
  function columnPx(): number {
    const parent = section.parentElement!;
    const cs = getComputedStyle(parent);
    return parent.clientHeight - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom);
  }
  function startDrag(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const grip = e.currentTarget as HTMLElement;
    grip.setPointerCapture(e.pointerId);
    const startY = e.clientY;
    const startH = section.getBoundingClientRect().height;
    const total = columnPx();
    const move = (m: PointerEvent) => setShare(((startH + startY - m.clientY) / total) * 100);
    const stop = () => {
      grip.removeEventListener('pointermove', move);
      grip.removeEventListener('pointerup', stop);
      grip.removeEventListener('pointercancel', stop);
    };
    grip.addEventListener('pointermove', move);
    grip.addEventListener('pointerup', stop);
    grip.addEventListener('pointercancel', stop);
  }
  function onGripKey(e: KeyboardEvent) {
    const now = share ?? (section.getBoundingClientRect().height / columnPx()) * 100;
    if (e.key === 'ArrowUp') setShare(now + KEY_STEP);
    else if (e.key === 'ArrowDown') setShare(now - KEY_STEP);
    else if (e.key === 'Home') setShare(null);
    else return;
    e.preventDefault();
  }

  function read(): boolean {
    try {
      return localStorage.getItem(`tray.${id}`) !== 'closed';
    } catch {
      return true;
    }
  }
  export function toggle() {
    open = !open;
    try {
      localStorage.setItem(`tray.${id}`, open ? 'open' : 'closed');
    } catch {
      // remembering the choice is a nicety
    }
  }
</script>

<section class="tray" class:open class:fixed class:sized={open && share !== null} style:--tray={accent} style:height={open && share !== null ? `${share}%` : undefined} bind:this={section}>
  {#if open}
    <button
      class="grip"
      aria-label="Resize {title} and the waterfall; arrow keys move the edge, Home resets"
      title="Drag to give {title} or the waterfall more room · double-click to reset"
      onpointerdown={startDrag}
      ondblclick={() => setShare(null)}
      onkeydown={onGripKey}
    ></button>
  {/if}
  <button class="bar" onclick={toggle} aria-expanded={open} title={open ? `Hide ${title}` : `Show ${title}`}>
    <span class="fold">{open ? '▾' : '▸'}</span>
    <span class="ttl">{title}</span>
    {#if summary}<span class="sum">{summary}</span>{/if}
  </button>
  {#if open}
    <div class="content">{@render children()}</div>
  {/if}
</section>

<style>
  .tray {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    border-radius: 10px;
    border: 1px solid var(--muted);
    background: var(--bg-light);
    min-height: 0;
  }
  .tray {
    position: relative;
  }
  .tray.open {
    max-height: 40%;
  }
  .tray.open.sized {
    max-height: none;
  }
  /* sits in the gap above the tray, so it never covers the bar */
  .grip {
    position: absolute;
    left: 0;
    right: 0;
    top: -10px;
    height: 10px;
    padding: 0;
    border: 0;
    background: transparent;
    cursor: row-resize;
    touch-action: none;
    z-index: 1;
  }
  .grip::after {
    content: '';
    position: absolute;
    left: 50%;
    top: 3px;
    width: 48px;
    height: 4px;
    margin-left: -24px;
    border-radius: 2px;
    background: var(--muted);
    opacity: 0;
    transition: opacity 0.15s;
  }
  .grip:hover::after,
  .grip:focus-visible::after,
  .tray:hover > .grip::after {
    opacity: 1;
  }
  .grip:hover::after,
  .grip:focus-visible::after {
    background: var(--tray);
  }
  .tray.open.fixed {
    height: 40%;
  }
  .bar {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-height: 34px;
    padding: 6px 12px;
    border: 0;
    background: transparent;
    text-align: left;
    min-width: 0;
  }
  .fold {
    color: var(--fg-dim);
    width: 1em;
  }
  .ttl {
    font-size: 14px;
    font-weight: 600;
    color: var(--tray);
    white-space: nowrap;
  }
  .sum {
    font-size: 13px;
    color: var(--fg-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .content {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 12px;
    padding: 0 12px 12px;
    overflow-y: auto;
    min-height: 0;
  }
  .fixed .content {
    flex: 1;
    flex-wrap: nowrap;
    overflow: hidden;
  }
</style>
