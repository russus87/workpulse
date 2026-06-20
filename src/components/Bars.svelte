<script>
  import { humanDuration } from "../lib/api.js";

  // Lista di righe { key, seconds } renderizzata come barre proporzionali.
  let { rows = [], green = false } = $props();

  let max = $derived(Math.max(1, ...rows.map((r) => r.seconds)));
</script>

{#if rows.length === 0}
  <p class="muted">Nessun dato per il periodo selezionato.</p>
{:else}
  {#each rows as r}
    <div class="bar-row">
      <div class="top">
        <span class="name">{r.key}</span>
        <span class="val">{humanDuration(r.seconds)}</span>
      </div>
      <div class="bar" class:green>
        <span style="width: {(r.seconds / max) * 100}%"></span>
      </div>
    </div>
  {/each}
{/if}
