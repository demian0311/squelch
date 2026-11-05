<script lang="ts">
  // The steps a receiver takes, drawn the same way on every screen: an icon in a ring, the step's words, a line to
  // the next. Done steps are green, the current one wears the band's colour, the rest wait in grey. One label per
  // step, short; the why goes in `title`.
  import Icon from './Icon.svelte';

  export type Step = { icon: string; label: string; state: 'done' | 'now' | 'todo'; title?: string };
  let { steps, label = 'What the receiver is doing' }: { steps: Step[]; label?: string } = $props();
</script>

<ol class="steps" aria-label={label}>
  {#each steps as s, i (i)}
    <li class="step {s.state}" title={s.title} aria-current={s.state === 'now' ? 'step' : undefined}>
      <span class="ring"><Icon name={s.state === 'done' ? 'check' : s.icon} size={14} /></span>
      <span class="words">{s.label}</span>
    </li>
  {/each}
</ol>

<style>
  .steps {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 0;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .step {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px 4px 5px;
    border-radius: 999px;
    border: 1px solid var(--muted);
    font-size: 13px;
    color: var(--fg-dim);
    white-space: nowrap;
  }
  /* the line to the next step */
  .step + .step {
    margin-left: 22px;
    position: relative;
  }
  .step + .step::before {
    content: '';
    position: absolute;
    left: -22px;
    top: 50%;
    width: 22px;
    border-top: 1px solid var(--muted);
  }
  .ring {
    display: inline-flex;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    align-items: center;
    justify-content: center;
    border: 1.5px solid var(--fg-dim);
    color: var(--fg-dim);
  }
  .step.done {
    color: var(--fg);
  }
  .step.done .ring {
    background: var(--green);
    border-color: var(--green);
    color: var(--bg);
  }
  .step.done + .step::before {
    border-color: var(--green);
  }
  .step.now {
    color: var(--fg-bright);
    border-color: var(--hue, var(--accent));
    background: color-mix(in srgb, var(--hue, var(--accent)) 14%, transparent);
  }
  .step.now .ring {
    border-color: var(--hue, var(--accent));
    color: var(--hue, var(--accent));
  }
</style>
