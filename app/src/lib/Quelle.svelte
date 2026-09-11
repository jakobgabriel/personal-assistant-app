<script lang="ts">
  // Detailseite einer Quelle: alle Signale, nicht nur die obersten drei.
  import * as api from "./api";
  import SignalZeile from "./SignalZeile.svelte";
  import { syncText } from "./format";
  import { app } from "./state.svelte";
  import { signalKey, sourceId, type Signal } from "./types";

  let { id, onZurueck }: { id: string; onZurueck: () => void } = $props();

  let signale = $state<Signal[]>([]);
  let laedt = $state(true);
  let fehler = $state<string | null>(null);

  const card = $derived(app.dashboard?.cards.find((c) => sourceId(c.source) === id));

  // Laedt neu, sobald eine andere Quelle gewaehlt wird oder ein Sync lief.
  $effect(() => {
    const quelle = id;
    const stand = app.dashboard?.now;
    void stand;
    laedt = true;
    api
      .getSignals(quelle)
      .then((s) => {
        signale = s;
        fehler = null;
      })
      .catch((e) => (fehler = String(e)))
      .finally(() => (laedt = false));
  });
</script>

<header>
  <button class="zurueck" onclick={onZurueck}>&larr;</button>
  <div>
    <h1>{card?.source.label ?? id}</h1>
    {#if card}
      <p class="leise stand">
        {syncText(card.sync)}
        {#if card.sync.fault}· {card.sync.fault.kind}{/if}
      </p>
    {/if}
  </div>
</header>

{#if laedt}
  <p class="leise">laedt …</p>
{:else if fehler}
  <p class="fehler">{fehler}</p>
{:else if signale.length === 0}
  <p class="leise">Nichts offen.</p>
{:else}
  {#each signale as signal (signalKey(signal))}
    <SignalZeile {signal} gross />
  {/each}
{/if}

<style>
  header {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 4px 0 16px;
  }

  h1 {
    font-size: 20px;
    font-weight: 600;
    margin: 0;
  }

  .stand {
    margin: 1px 0 0;
    font-size: 12px;
  }

  .zurueck {
    font-size: 20px;
    width: 28px;
    color: var(--text-leise);
  }

  .fehler {
    color: var(--kritisch);
  }
</style>
