<script lang="ts">
  // Eingabefeld fuer ein Geheimnis. Der Wert geht in eine Richtung: hinein.
  // Wieder heraus kommt nur, ob etwas hinterlegt ist.
  import { app } from "./state.svelte";

  let { name, label }: { name: string; label: string } = $props();

  let wert = $state("");
  let gespeichert = $state(false);
  let fehler = $state<string | null>(null);

  const vorhanden = $derived(app.hatSecret(name));

  async function speichern() {
    if (!wert) return;
    try {
      await app.setSecret(name, wert);
      wert = "";
      gespeichert = true;
      fehler = null;
      setTimeout(() => (gespeichert = false), 2000);
    } catch (e) {
      fehler = String(e);
    }
  }
</script>

<label class="feld">
  <span class="label">{label}</span>
  <div class="zeile">
    <input
      type="password"
      bind:value={wert}
      autocomplete="off"
      spellcheck="false"
      placeholder={vorhanden ? "hinterlegt — zum Ersetzen eingeben" : "noch nichts hinterlegt"} />
    <button onclick={speichern} disabled={!wert}>Sichern</button>
  </div>
  <span class="hinweis" class:gut={vorhanden || gespeichert}>
    {#if gespeichert}gespeichert{:else if vorhanden}hinterlegt{:else}fehlt{/if}
    · Schluessel <code>{name}</code>
  </span>
  {#if fehler}<span class="fehler">{fehler}</span>{/if}
</label>

<style>
  .feld {
    display: block;
    margin-bottom: 12px;
  }

  .zeile {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .zeile button {
    flex: 0 0 auto;
    border: 1px solid var(--linie);
    border-radius: 6px;
    padding: 0 14px;
    font-size: 13px;
  }

  .hinweis {
    display: block;
    font-size: 11px;
    color: var(--text-sehr-leise);
    margin-top: 3px;
  }

  .hinweis.gut {
    color: var(--akzent);
  }

  code {
    font-family: var(--mono);
    font-size: 10px;
  }

  .fehler {
    display: block;
    color: var(--kritisch);
    font-size: 12px;
  }
</style>
