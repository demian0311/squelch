<script lang="ts">
  // Signal strength: green to yellow up to `pct` of the way, the rest dimmed.
  let { pct, width = '44px', height = '6px' }: { pct: number; width?: string; height?: string } = $props();
</script>

<span class="level" style:width style:height style:--fill="{Math.max(0, Math.min(100, pct))}%"></span>

<style>
  .level {
    flex: none;
    display: block;
    border-radius: 3px;
    background: linear-gradient(90deg, var(--green), var(--yellow)), var(--muted);
    -webkit-mask: linear-gradient(90deg, #000 var(--fill), rgb(0 0 0 / 25%) var(--fill));
    mask: linear-gradient(90deg, #000 var(--fill), rgb(0 0 0 / 25%) var(--fill));
  }
</style>
