<script lang="ts">
  // Band list: groups that fold, and a narrow icon-only form.
  import type { Band, BandPlan } from '../lib/api';
  import Icon from './Icon.svelte';

  interface Props {
    plan: BandPlan;
    current: string | null;
    open: boolean;
    onPick: (b: Band) => void;
    onToggle: () => void;
    /** Per band id: `live` puts a pulsing dot on the icon; `title` replaces the tooltip. */
    marks?: Record<string, { live: boolean; title: string }>;
  }
  let { plan, current, open, onPick, onToggle, marks = {} }: Props = $props();

  let folded = $state<Record<string, boolean>>({});

  let groups = $derived.by(() => {
    let key = 0;
    return plan.groups.map((g) => ({
      ...g,
      bands: plan.bands
        .filter((b) => b.group === g.id)
        .map((b) => ({ band: b, key: key < 9 ? String(++key) : '' })),
    }));
  });

  function range(b: Band) {
    const f = (v: number) => (v >= 100 ? v.toFixed(0) : v.toFixed(v % 1 === 0 ? 0 : 2));
    return `${f(b.low_mhz)}–${f(b.high_mhz)} MHz`;
  }
</script>

<nav aria-label="Bands" class:closed={!open}>
  <button class="toggle" onclick={onToggle} aria-label={open ? 'Collapse band list' : 'Expand band list'} title={open ? 'Collapse band list' : 'Expand band list'}>
    {open ? '«' : '»'}
  </button>
  {#each groups as g (g.id)}
    <div class="group">
      {#if open}
        <button class="head" onclick={() => (folded[g.id] = !folded[g.id])} aria-expanded={!folded[g.id]}>
          <span class="chev">{folded[g.id] ? '▸' : '▾'}</span>{g.name}
        </button>
      {:else}
        <div class="divider"></div>
      {/if}
      {#each g.bands as { band, key } (band.id)}
        {#if !open || !folded[g.id] || band.id === current}
          <button class="band" class:sel={band.id === current} onclick={() => onPick(band)} title={marks[band.id] ? `${marks[band.id].title} · ${range(band)}` : `${band.name} · ${range(band)}`} aria-label={marks[band.id]?.live ? `${band.name}, overhead now` : band.name}>
            <span class="icon"><Icon name={band.icon} />{#if marks[band.id]?.live}<span class="live"></span>{/if}</span>
            {#if open}
              <span class="text">
                <span class="name">{band.name}</span>
                <span class="range">{range(band)}</span>
              </span>
              <span class="key mono">{key}</span>
            {/if}
          </button>
        {/if}
      {/each}
    </div>
  {/each}
</nav>

<style>
  nav {
    width: 248px;
    flex: 0 0 248px;
    padding: 10px 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--bg-dark);
    border-right: 1px solid var(--muted);
    overflow-y: auto;
  }
  nav.closed {
    width: 68px;
    flex-basis: 68px;
  }
  .toggle {
    align-self: flex-end;
    min-height: 32px;
    min-width: 44px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--fg-dim);
    font-size: 16px;
  }
  nav.closed .toggle {
    align-self: center;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 6px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 30px;
    padding: 0 10px;
    border: 0;
    background: transparent;
    color: var(--fg-dim);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
    text-align: left;
  }
  .chev {
    width: 12px;
  }
  .divider {
    height: 1px;
    margin: 4px 10px;
    background: var(--muted);
  }
  .band {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 4px 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--fg);
    text-align: left;
  }
  nav.closed .band {
    justify-content: center;
  }
  .band:hover {
    background: var(--bg-light);
  }
  .band.sel {
    background: var(--sel);
    color: var(--fg-bright);
  }
  .icon {
    display: flex;
    color: var(--fg-dim);
  }
  .sel .icon {
    color: var(--accent);
  }
  .icon {
    position: relative;
  }
  .live {
    position: absolute;
    top: -3px;
    right: -4px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--green);
    box-shadow: 0 0 0 2px var(--bg-dark);
    animation: pulse 1.6s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
  .text {
    display: flex;
    flex-direction: column;
    flex-grow: 1;
    min-width: 0;
  }
  .name {
    font-weight: 500;
  }
  .range {
    font-size: 12px;
    color: var(--fg-dim);
  }
  .key {
    font-size: 11px;
    color: var(--fg-dim);
  }
</style>
