<script lang="ts">
  // Der Startscreen: Gruss, was jetzt zaehlt, dann der Rest, dann die Karten.
  import * as api from "./api";
  import Karte from "./Karte.svelte";
  import SignalZeile from "./SignalZeile.svelte";
  import { gruss, heuteLang } from "./format";
  import { app } from "./state.svelte";
  import { signalKey } from "./types";

  let { onOeffnen }: { onOeffnen: (id: string) => void } = $props();

  let briefing = $state<string | null>(null);
  let briefingLaeuft = $state(false);
  let briefingFehler = $state<string | null>(null);

  const dash = $derived(app.dashboard);
  const kiAn = $derived(app.config?.ai.enabled ?? false);

  async function briefHolen() {
    briefingLaeuft = true;
    briefingFehler = null;
    try {
      briefing = (await api.dailyBrief()).text;
    } catch (e) {
      briefingFehler = String(e);
    } finally {
      briefingLaeuft = false;
    }
  }
</script>

<header>
  <h1>{gruss()}</h1>
  <p class="datum leise">{heuteLang()}</p>
</header>

{#if dash}
  {#each dash.alerts as alert (alert.source.instance + alert.message)}
    <div class="banner" class:handeln={alert.needs_action}>
      <div>
        <strong>{alert.source.label}</strong>
        <span class="leise">{alert.message}</span>
      </div>
      {#if alert.needs_action}
        <button onclick={() => onOeffnen("einstellungen")}>Richten</button>
      {/if}
    </div>
  {/each}

  {#if kiAn}
    <section class="briefing">
      {#if briefing}
        <p>{briefing}</p>
      {:else if briefingFehler}
        <p class="fehler">{briefingFehler}</p>
      {:else}
        <button class="briefing-knopf" onclick={briefHolen} disabled={briefingLaeuft}>
          {briefingLaeuft ? "denkt nach …" : "Tagesbriefing erzeugen"}
        </button>
      {/if}
    </section>
  {/if}

  {#if dash.top.length > 0}
    <section>
      <h2 class="label">Jetzt wichtig</h2>
      {#each dash.top as signal (signalKey(signal))}
        <SignalZeile {signal} gross />
      {/each}
    </section>
  {/if}

  {#if dash.cards.length > 0}
    <section>
      <h2 class="label">Quellen</h2>
      <div class="karten">
        {#each dash.cards as card (card.source.kind + card.source.instance)}
          <Karte {card} {onOeffnen} />
        {/each}
      </div>
    </section>
  {/if}

  {#if dash.rest.length > 0}
    <section>
      <h2 class="label">Weiter</h2>
      {#each dash.rest.slice(0, 40) as signal (signalKey(signal))}
        <SignalZeile {signal} />
      {/each}
    </section>
  {/if}

  {#if dash.cards.length === 0}
    <section class="leerer-start">
      <p>Noch keine Quelle eingerichtet.</p>
      <p class="leise">
        Obsidian-Vaults, Mindwtr, NocoDB, Gmail und Nachrichtenfeeds richtest du in den
        Einstellungen ein.
      </p>
      <button class="primaer" onclick={() => onOeffnen("einstellungen")}>
        Zu den Einstellungen
      </button>
    </section>
  {:else if dash.top.length === 0 && dash.rest.length === 0}
    <section class="leerer-start">
      <p>Nichts, was heute Aufmerksamkeit braucht.</p>
    </section>
  {/if}
{/if}

<style>
  header {
    padding: 4px 0 14px;
  }

  h1 {
    font-size: 23px;
    font-weight: 600;
    margin: 0;
  }

  .datum {
    margin: 2px 0 0;
    font-size: 13px;
  }

  section {
    margin-bottom: 22px;
  }

  h2 {
    margin: 0 0 6px;
  }

  .karten {
    display: grid;
    gap: 10px;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  }

  .banner {
    display: flex;
    gap: 10px;
    align-items: center;
    justify-content: space-between;
    background: var(--flaeche);
    border: 1px solid var(--linie);
    border-left: 3px solid var(--hoch);
    border-radius: var(--radius);
    padding: 9px 12px;
    margin-bottom: 10px;
    font-size: 12px;
  }

  .banner.handeln {
    border-left-color: var(--kritisch);
  }

  .banner button {
    flex: 0 0 auto;
    border: 1px solid var(--linie);
    border-radius: 999px;
    padding: 4px 12px;
    font-size: 12px;
  }

  .briefing {
    background: var(--flaeche);
    border: 1px solid var(--linie);
    border-radius: var(--radius);
    padding: 12px 14px;
    margin-bottom: 20px;
  }

  .briefing p {
    margin: 0;
    font-size: 13px;
  }

  .briefing-knopf {
    color: var(--akzent);
    font-size: 13px;
    padding: 0;
  }

  .fehler {
    color: var(--kritisch);
  }

  .leerer-start {
    text-align: center;
    padding: 36px 12px;
  }

  .leerer-start p {
    margin: 0 0 8px;
  }

  .primaer {
    margin-top: 12px;
    background: var(--akzent);
    color: var(--grund);
    border-radius: 999px;
    padding: 9px 20px;
    font-weight: 600;
  }
</style>
