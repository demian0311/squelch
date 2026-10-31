<script lang="ts">
  import type { Snippet } from 'svelte';

  // A panel under the waterfall that folds to one line. Open or closed is remembered per `id`.
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

<section class="tray" class:open class:fixed style:--tray={accent}>
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
  .tray.open {
    max-height: 40%;
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
