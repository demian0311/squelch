<script lang="ts">
  // The one layout for a map beside rows: the map where the waterfall sits, the rows where the
  // panel sits, a divider the user drags, rows below the map once the split is narrower than
  // STACK_UNDER px. The same shape as App's main split, so every screen reads like every band.
  import type { Snippet } from 'svelte';

  interface Props {
    /** the rows' share of the width (height when stacked), 20–75 */
    share: number;
    /** called with the new share while the divider is dragged, and with `save` once it is let go */
    onShare: (share: number, save: boolean) => void;
    map: Snippet;
    rows: Snippet;
  }
  let { share, onShare, map, rows }: Props = $props();

  const STACK_UNDER = 1000;
  const MIN_SHARE = 20;
  const MAX_SHARE = 75;

  let width = $state(0);
  let stacked = $derived(width > 0 && width < STACK_UNDER);
  let el: HTMLElement;

  function startDivider(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const grip = e.currentTarget as HTMLElement;
    grip.setPointerCapture(e.pointerId);
    const r = el.getBoundingClientRect();
    const move = (m: PointerEvent) => {
      const v = stacked ? ((r.bottom - m.clientY) / r.height) * 100 : ((r.right - m.clientX) / r.width) * 100;
      onShare(Math.round(Math.min(MAX_SHARE, Math.max(MIN_SHARE, v))), false);
    };
    const stop = () => {
      grip.removeEventListener('pointermove', move);
      grip.removeEventListener('pointerup', stop);
      onShare(share, true);
    };
    grip.addEventListener('pointermove', move);
    grip.addEventListener('pointerup', stop);
  }
</script>

<div class="split" class:stacked bind:this={el} bind:clientWidth={width} style:--share="{share}%">
  <div class="cell map">{@render map()}</div>
  <button class="divider" aria-label="Resize the rows" title="Drag to give the rows or the map more room" onpointerdown={startDivider}></button>
  <div class="cell rows">{@render rows()}</div>
</div>

<style>
  .split {
    flex: 1;
    min-height: 0;
    min-width: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 10px minmax(280px, var(--share));
    grid-template-rows: minmax(0, 1fr);
  }
  .split.stacked {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) 10px minmax(120px, var(--share));
  }
  /* a cell stretches its one child to fill it */
  .cell {
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    grid-template-columns: minmax(0, 1fr);
    min-height: 0;
    min-width: 0;
  }
  .divider {
    position: relative;
    padding: 0;
    border: 0;
    background: transparent;
    cursor: col-resize;
    touch-action: none;
  }
  .stacked .divider {
    cursor: row-resize;
  }
  .divider::after {
    content: '';
    position: absolute;
    inset: 0;
    margin: auto;
    width: 4px;
    height: 48px;
    border-radius: 2px;
    background: var(--muted);
    opacity: 0;
    transition: opacity 0.15s;
  }
  .stacked .divider::after {
    width: 48px;
    height: 4px;
  }
  .divider:hover::after,
  .divider:active::after,
  .divider:focus-visible::after {
    opacity: 1;
  }
</style>
