<script lang="ts">
  // Eine Zeile im Startscreen. Sie kennt keine Domaene — nur `Signal`.
  import * as api from "./api";
  import { zeitText } from "./format";
  import { app } from "./state.svelte";
  import { signalKey, type Signal } from "./types";

  let { signal, gross = false }: { signal: Signal; gross?: boolean } = $props();

  let offen = $state(false);
  let laeuft = $state(false);
  let fehler = $state<string | null>(null);

  const zeit = $derived(zeitText(signal));
  const key = $derived(signalKey(signal));

  async function ausfuehren() {
    if (!signal.action) {
      offen = !offen;
      return;
    }
    laeuft = true;
    fehler = null;
    try {
      await api.runAction(signal.action);
      // Abhaken aendert die Quelle — der Startscreen muss das sehen.
      if (signal.action.kind === "complete_task") await app.reload();
    } catch (e) {
      fehler = String(e);
    } finally {
      laeuft = false;
    }
  }

  async function spaeter(stunden: number) {
    await api.snoozeSignal(key, stunden);
    await app.reload();
  }

  async function wegwischen() {
    await api.dismissSignal(key);
    await app.reload();
  }
</script>

<div class="zeile u-{signal.urgency}" class:gross>
  <div class="balken"></div>

  <button class="haupt" onclick={ausfuehren} disabled={laeuft}>
    <div class="kopf">
      <span class="titel">{signal.title}</span>
      {#if zeit}<span class="zeit zahl">{zeit}</span>{/if}
    </div>
    {#if signal.subtitle}
      <div class="unter leise">{signal.subtitle}</div>
    {/if}
    {#if gross && signal.excerpt}
      <div class="auszug leise">{signal.excerpt}</div>
    {/if}
  </button>

  <button
    class="mehr"
    onclick={() => (offen = !offen)}
    aria-label={offen ? "Weniger" : "Mehr"}
    aria-expanded={offen}>{offen ? "×" : "⋯"}</button>
</div>

{#if offen}
  <div class="aktionen fade">
    <button onclick={() => spaeter(3)}>3 h spaeter</button>
    <button onclick={() => spaeter(24)}>Morgen</button>
    <button onclick={wegwischen}>Erledigt</button>
    {#if signal.badge}<span class="badge label">{signal.badge}</span>{/if}
  </div>
{/if}

{#if fehler}
  <div class="fehler">{fehler}</div>
{/if}

<style>
  .zeile {
    display: flex;
    gap: 10px;
    align-items: stretch;
    padding: 10px 0;
    border-bottom: 1px solid var(--linie);
  }

  .haupt {
    flex: 1;
    text-align: left;
    padding: 0;
    min-width: 0;
  }

  .kopf {
    display: flex;
    gap: 10px;
    align-items: baseline;
  }

  .titel {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .gross .titel {
    white-space: normal;
    font-size: 15px;
  }

  .zeit {
    flex: 0 0 auto;
    font-size: 11px;
    color: var(--u);
  }

  .unter,
  .auszug {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .auszug {
    white-space: normal;
    margin-top: 3px;
    /* Zwei Zeilen Vorschau; alles Weitere gehoert in die Quelle. */
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .mehr {
    flex: 0 0 auto;
    width: 32px;
    color: var(--text-sehr-leise);
    font-size: 16px;
    line-height: 1;
  }

  .aktionen {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
    padding: 8px 0 10px 13px;
    border-bottom: 1px solid var(--linie);
  }

  .aktionen button {
    font-size: 12px;
    padding: 5px 10px;
    border: 1px solid var(--linie);
    border-radius: 999px;
    color: var(--text-leise);
  }

  .badge {
    margin-left: auto;
  }

  .fehler {
    color: var(--kritisch);
    font-size: 12px;
    padding: 6px 0 8px 13px;
  }
</style>
