<script lang="ts">
  // One category's guide, covering `main` under the top bar; the radio keeps playing. Rendered from the markdown in
  // `ui/src/guides/` (see `lib/guide.ts`). Links do things: `tune:` tunes, `band:` opens a rail entry, `https://` opens
  // the browser.
  import type { Guide } from '../lib/guide';
  import type { Mode } from '../lib/api';
  import Icon from './Icon.svelte';

  interface Props {
    guide: Guide;
    /** The group's colour: the heading and the diagrams' accent. */
    hue: string;
    onTune: (hz: number, mode: Mode | null) => void;
    onBand: (id: string) => void;
    onOpen: (url: string) => void;
    onClose: () => void;
  }
  let { guide, hue, onTune, onBand, onOpen, onClose }: Props = $props();

  function click(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest('a[data-act]') as HTMLAnchorElement | null;
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute('href') ?? '';
    if (href.startsWith('tune:')) {
      const [mhz, mode] = href.slice(5).split('/');
      onTune(Math.round(parseFloat(mhz) * 1e6), (mode as Mode | undefined) ?? null);
    } else if (href.startsWith('band:')) onBand(href.slice(5));
    else if (href.startsWith('https://')) onOpen(href);
  }
</script>

<section class="screen" style:--c={hue} aria-label="About {guide.title}">
  <div class="head">
    <h1><span class="swatch"></span>{guide.title}</h1>
    <button class="close" onclick={onClose} title="Back (Esc)" aria-label="Close the guide"><Icon name="close" size={16} /></button>
  </div>
  <!-- the links inside are the interactive parts; the handler only delegates to them -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="body" onclick={click}>{@html guide.html}</div>
</section>

<style>
  .screen {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 20px 16px;
    background: var(--bg);
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .head {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
  }
  h1 {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--fg-bright);
  }
  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 3px;
    background: var(--c);
  }
  .close {
    margin-left: auto;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--fg-dim);
    cursor: pointer;
  }
  .close:hover {
    color: var(--fg);
    background: var(--bg-light);
  }
  /* the sections flow into columns; one screen on a wide window, a scroll on a narrow one */
  .body {
    columns: 3 300px;
    column-gap: 28px;
    font-size: 13.5px;
    line-height: 1.45;
    color: var(--fg);
  }
  .body :global(section) {
    break-inside: avoid;
    margin: 0 0 14px;
  }
  .body :global(.lead) {
    column-span: all;
    font-size: 15px;
    color: var(--fg-bright);
    max-width: 90ch;
  }
  .body :global(h2) {
    margin: 0 0 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--c);
  }
  .body :global(p) {
    margin: 0 0 8px;
  }
  .body :global(ul) {
    margin: 0 0 8px;
    padding-left: 18px;
  }
  .body :global(li) {
    margin-bottom: 4px;
  }
  .body :global(table) {
    border-collapse: collapse;
    width: 100%;
    margin: 0 0 8px;
    font-size: 12.5px;
  }
  .body :global(th) {
    text-align: left;
    font-weight: 500;
    color: var(--fg-dim);
    padding: 0 8px 3px 0;
    border-bottom: 1px solid var(--muted);
  }
  .body :global(td) {
    padding: 3px 8px 3px 0;
    vertical-align: top;
    border-bottom: 1px solid color-mix(in srgb, var(--muted) 50%, transparent);
  }
  .body :global(td:first-child) {
    width: 36%;
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
  }
  .body :global(td:last-child) {
    color: var(--fg-dim);
  }
  .body :global(code) {
    font-family: var(--mono);
    font-size: 12px;
    background: var(--bg-light);
    padding: 0 4px;
    border-radius: 3px;
  }
  .body :global(figure) {
    margin: 4px 0 10px;
    color: var(--fg-dim);
  }
  .body :global(figure svg) {
    display: block;
    width: 100%;
    height: auto;
    max-width: 400px;
    font-family: var(--sans);
  }
  .body :global(figcaption) {
    font-size: 12px;
    margin-top: 2px;
  }
  .body :global(a) {
    color: var(--c);
    text-decoration: none;
    border-bottom: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
  }
  .body :global(a[data-act='tune']),
  .body :global(a[data-act='band']) {
    display: inline-block;
    border: 1px solid var(--muted);
    border-radius: 6px;
    padding: 2px 8px;
    margin: 2px 6px 2px 0;
    color: var(--fg);
    background: var(--bg-light);
  }
  .body :global(a[data-act='tune']:hover),
  .body :global(a[data-act='band']:hover) {
    border-color: var(--c);
  }
</style>
