<script lang="ts">
  // Band list: groups that fold, a narrow icon-only form, and the settings drawer that slides over it.
  import type { Snippet } from 'svelte';
  import type { Band, BandPlan } from '../lib/api';
  import Icon from './Icon.svelte';

  interface Props {
    plan: BandPlan;
    current: string | null;
    /** Tuned outside the current band: its entry is shown hollow. */
    outside?: boolean;
    open: boolean;
    onPick: (b: Band) => void;
    onToggle: () => void;
    /** Colour per group id. */
    hues: Record<string, string>;
    /** Per band id: `live` puts a pulsing dot on the icon; `title` replaces the tooltip; `line` replaces the frequency range. */
    marks?: Record<string, { live: boolean; title: string; line?: string }>;
    settingsOpen: boolean;
    onSettings: () => void;
    settings: Snippet;
  }
  let { plan, current, outside = false, open, onPick, onToggle, hues, marks = {}, settingsOpen, onSettings, settings }: Props = $props();

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
  // "2 m", "70 cm": the name says more than an icon five bands share
  const short = (b: Band) => (/^[\d.]+ c?m$/.test(b.name) ? b.name.replace(' ', '') : null);
</script>

<nav aria-label="Bands" class:closed={!open}>
  <div class="list">
    {#each groups as g (g.id)}
      <div class="group" style:--c={hues[g.id]}>
        {#if open}
          <button class="head" onclick={() => (folded[g.id] = !folded[g.id])} aria-expanded={!folded[g.id]}>
            <span class="swatch"></span>{g.name}<span class="chev"><Icon name={folded[g.id] ? 'chevronRight' : 'chevronDown'} size={14} /></span>
          </button>
        {:else}
          <div class="divider"></div>
        {/if}
        {#each g.bands as { band, key } (band.id)}
          {#if !open || !folded[g.id] || band.id === current}
            <button class="band" class:sel={band.id === current} class:outside={band.id === current && outside} onclick={() => onPick(band)} title={marks[band.id] ? `${marks[band.id].title} · ${range(band)}` : `${band.name} · ${range(band)}`} aria-label={marks[band.id]?.live ? `${band.name}, overhead now` : band.name}>
              <span class="icon" class:txt={short(band)}>
                {#if short(band)}{short(band)}{:else}<Icon name={band.icon} size={18} />{/if}
                {#if marks[band.id]?.live}<span class="live"></span>{/if}
              </span>
              {#if open}
                <span class="text">
                  <span class="name">{band.name}</span>
                  <span class="range mono">{marks[band.id]?.line ?? range(band)}</span>
                </span>
                <span class="key mono">{key}</span>
              {/if}
            </button>
          {/if}
        {/each}
      </div>
    {/each}
  </div>

  <div class="drawer" class:shown={settingsOpen} inert={!settingsOpen}>
    <div class="dhead">
      <span>Settings</span>
      <button onclick={onSettings} aria-label="Close settings" title="Close settings (S)">×</button>
    </div>
    {@render settings()}
  </div>

  <div class="foot">
    <button class="gear" class:on={settingsOpen} onclick={onSettings} title="Settings (S)" aria-expanded={settingsOpen}>
      <span class="glyph">⚙</span>{#if open}<span>Settings</span>{/if}
    </button>
    <button class="toggle" onclick={onToggle} aria-label={open ? 'Narrow the band list' : 'Widen the band list'} title={open ? 'Narrow the band list' : 'Widen the band list'}>
      <Icon name={open ? 'sidebarClose' : 'sidebarOpen'} size={18} />
    </button>
  </div>
</nav>

<style>
  nav {
    position: relative;
    width: 236px;
    flex: 0 0 236px;
    display: flex;
    flex-direction: column;
    background: var(--bg-dark);
    border-right: 1px solid var(--muted);
    min-height: 0;
  }
  nav.closed {
    width: 64px;
    flex-basis: 64px;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin-bottom: 6px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
    padding: 0 10px;
    border: 0;
    background: transparent;
    color: var(--fg-dim);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
    text-align: left;
  }
  .swatch {
    width: 7px;
    height: 7px;
    border-radius: 2px;
    background: var(--c);
  }
  .chev {
    display: flex;
    margin-left: auto;
    opacity: 0.5;
    transition: opacity 0.15s;
  }
  .head:hover .chev,
  .head:focus-visible .chev {
    opacity: 1;
  }
  .divider {
    height: 2px;
    margin: 6px 14px;
    border-radius: 1px;
    background: color-mix(in srgb, var(--c) 45%, transparent);
  }
  .band {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 42px;
    padding: 4px 8px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--fg);
    text-align: left;
  }
  nav.closed .band {
    justify-content: center;
    padding: 4px 0;
  }
  .band:hover {
    background: var(--bg-light);
  }
  .band.sel {
    background: color-mix(in srgb, var(--c) 16%, var(--bg-dark));
    color: var(--fg-bright);
  }
  .band.sel.outside {
    background: transparent;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--c) 60%, transparent);
  }
  .icon {
    position: relative;
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 14%, transparent);
  }
  .icon.txt {
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: -0.3px;
  }
  .sel .icon {
    background: var(--c);
    color: var(--bg-darker);
  }
  .live {
    position: absolute;
    top: -3px;
    right: -3px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--yellow);
    /* steady: a running animation redraws the whole window 60 times a second here */
    box-shadow: 0 0 0 2px var(--bg-dark);
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
    font-size: 11px;
    color: var(--fg-dim);
  }
  .key {
    font-size: 11px;
    color: var(--fg-dim);
  }
  .foot {
    display: flex;
    gap: 4px;
    padding: 8px;
    border-top: 1px solid var(--muted);
  }
  nav.closed .foot {
    flex-direction: column;
  }
  .foot button {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    min-height: 36px;
    min-width: 40px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--fg-dim);
  }
  .foot .gear {
    flex: 1;
    justify-content: flex-start;
  }
  nav.closed .foot .gear {
    justify-content: center;
  }
  .foot button:hover,
  .foot .gear.on {
    background: var(--bg-light);
    color: var(--fg-bright);
  }
  .glyph {
    font-size: 16px;
  }
  /* slides over the band list, above the footer; wider than the list when the rail is narrow */
  .drawer {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 53px;
    width: 100%;
    z-index: 5;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 14px 16px 18px;
    overflow-y: auto;
    background: var(--bg-dark);
    border-right: 1px solid var(--muted);
    transform: translateX(-102%);
    transition: transform 0.2s ease;
  }
  nav.closed .drawer {
    width: 280px;
    bottom: 97px;
    box-shadow: 10px 0 30px rgb(0 0 0 / 0.35);
  }
  .drawer.shown {
    transform: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .drawer {
      transition: none;
    }
  }
  .dhead {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
    font-size: 15px;
    color: var(--fg-bright);
  }
  .dhead button {
    border: 0;
    background: transparent;
    color: var(--fg-dim);
    font-size: 20px;
    min-width: 32px;
    min-height: 32px;
    border-radius: 6px;
  }
  .dhead button:hover {
    background: var(--bg-light);
  }
</style>
