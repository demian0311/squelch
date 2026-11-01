<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  // A titled panel. `onMax` adds the give-it-the-screen button and makes a double-click on the title do the same.
  // `mini` folds it to the title line, for the panel that gave its room away.
  let {
    title,
    summary = '',
    accent = 'var(--accent)',
    maxed = false,
    mini = false,
    onMax,
    tabs,
    tools,
    children,
  }: {
    title: string;
    summary?: string;
    accent?: string;
    maxed?: boolean;
    mini?: boolean;
    onMax?: () => void;
    tabs?: Snippet;
    tools?: Snippet;
    children: Snippet;
  } = $props();
</script>

<section class="pane" class:mini style:--pane={accent}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="bar" ondblclick={(e) => onMax && !(e.target as HTMLElement).closest('button') && onMax()}>
    <span class="ttl"><span class="swatch"></span>{title}</span>
    {#if tabs}{@render tabs()}{/if}
    {#if summary}<span class="sum" title={summary}>{summary}</span>{:else}<span class="sum"></span>{/if}
    <span class="tools">
      {#if tools && !mini}{@render tools()}{/if}
      {#if onMax}
        <button class="max" onclick={onMax} title={maxed || mini ? 'Restore (F)' : `Give ${title} the screen (F)`} aria-label={maxed || mini ? 'Restore' : `Give ${title} the screen`}><Icon name={maxed || mini ? 'restore' : 'maximize'} size={16} /></button>
      {/if}
    </span>
  </div>
  {#if !mini}
    <div class="content">{@render children()}</div>
  {/if}
</section>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    border-radius: 10px;
    border: 1px solid var(--muted);
    background: var(--bg-dark);
    overflow: hidden;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 36px;
    padding: 4px 6px 4px 12px;
    border-bottom: 1px solid var(--muted);
    min-width: 0;
    user-select: none;
  }
  .mini .bar {
    border-bottom: 0;
  }
  .ttl {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    font-weight: 600;
    color: var(--fg-bright);
    white-space: nowrap;
  }
  .swatch {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--pane);
  }
  .sum {
    flex: 1;
    font-size: 12px;
    color: var(--fg-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  /* view tools stay faint until the panel is in use */
  .tools {
    display: flex;
    align-items: center;
    gap: 2px;
    opacity: 0.55;
    transition: opacity 0.15s;
  }
  .pane:hover .tools,
  .tools:focus-within {
    opacity: 1;
  }
  .tools :global(button) {
    min-height: 28px;
    min-width: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--fg-dim);
    font-size: 12px;
    white-space: nowrap;
  }
  .tools :global(button:hover) {
    background: var(--bg-light);
    color: var(--fg-bright);
  }
  .tools :global(button.on) {
    color: var(--pane);
  }
  .max {
    display: grid;
    place-items: center;
  }
  .content {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    container-type: inline-size;
  }
</style>
