<script lang="ts">
  // The corner of a map: closer, wider, and on a map that follows its data, back to following it.
  import { BUTTON_STEP, type MapLimits } from '../lib/mapview';
  import Icon from './Icon.svelte';

  interface Props {
    halfKm: number;
    limits: MapLimits;
    onZoom: (factor: number) => void;
    /** the map can frame its data by itself: `on` while it does */
    fit?: { on: boolean; title: string; onFit: () => void };
  }
  let { halfKm, limits, onZoom, fit }: Props = $props();
</script>

<div class="zoom">
  <button type="button" aria-label="Closer" title="Closer (or scroll on the map)" disabled={halfKm <= limits.nearestKm} onclick={() => onZoom(BUTTON_STEP)}>+</button>
  <button type="button" aria-label="Wider" title="Wider (or scroll on the map)" disabled={halfKm >= limits.widestKm} onclick={() => onZoom(1 / BUTTON_STEP)}>−</button>
  {#if fit}
    <button type="button" class="fit" aria-label={fit.title} title={fit.title} disabled={fit.on} onclick={fit.onFit}><Icon name="shrink" size={14} /></button>
  {/if}
</div>

<style>
  .zoom {
    position: absolute;
    right: 8px;
    bottom: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  button {
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--muted);
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--bg-darker) 85%, transparent);
    color: var(--fg);
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
    display: grid;
    place-items: center;
  }
  button:disabled {
    color: var(--muted);
    cursor: default;
  }
  button:focus-visible {
    outline: 2px solid var(--accent);
  }
</style>
