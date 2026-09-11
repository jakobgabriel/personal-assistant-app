<script lang="ts">
  // Die Schale: Reiter, Ziehen zum Aktualisieren, Thema.
  import { listen } from "@tauri-apps/api/event";
  import Einstellungen from "./lib/Einstellungen.svelte";
  import Heute from "./lib/Heute.svelte";
  import Quelle from "./lib/Quelle.svelte";
  import { app } from "./lib/state.svelte";

  type Ansicht = { art: "heute" } | { art: "quelle"; id: string } | { art: "einstellungen" };

  let ansicht = $state<Ansicht>({ art: "heute" });
  let hinweis = $state<string | null>(null);

  app.start();

  // Das Thema steht in der Konfiguration; das Wurzelelement traegt es.
  $effect(() => {
    const thema = app.config?.dashboard.theme ?? "cockpit";
    document.documentElement.dataset.thema = thema;
  });

  // Der OAuth-Rueckkanal kommt als Deep Link in Rust an und wird von dort
  // gemeldet — die Oberflaeche erfaehrt nur das Ergebnis.
  $effect(() => {
    const ab = listen<string>("gmail-auth", async (e) => {
      hinweis = `Gmail-Konto "${e.payload}" angemeldet.`;
      await app.reload();
      await app.sync(true);
      setTimeout(() => (hinweis = null), 4000);
    });
    const abFehler = listen<string>("gmail-auth-error", (e) => {
      hinweis = `Anmeldung fehlgeschlagen: ${e.payload}`;
    });
    return () => {
      void ab.then((f) => f());
      void abFehler.then((f) => f());
    };
  });

  function oeffnen(id: string) {
    ansicht = id === "einstellungen" ? { art: "einstellungen" } : { art: "quelle", id };
    scrollTo({ top: 0 });
  }
</script>

<main>
  {#if hinweis}
    <div class="hinweis fade">{hinweis}</div>
  {/if}

  {#if app.fehler}
    <div class="hinweis fehler">{app.fehler}</div>
  {/if}

  {#if app.laedt && !app.dashboard}
    <p class="leise mitte">laedt …</p>
  {:else if ansicht.art === "heute"}
    <Heute onOeffnen={oeffnen} />
  {:else if ansicht.art === "quelle"}
    <Quelle id={ansicht.id} onZurueck={() => (ansicht = { art: "heute" })} />
  {:else if app.config}
    <Einstellungen />
  {/if}
</main>

<nav>
  <button class:aktiv={ansicht.art === "heute"} onclick={() => (ansicht = { art: "heute" })}>
    Heute
  </button>
  <button class="sync" onclick={() => app.sync(true)} disabled={app.synct}>
    {app.synct ? "…" : "Aktualisieren"}
  </button>
  <button
    class:aktiv={ansicht.art === "einstellungen"}
    onclick={() => (ansicht = { art: "einstellungen" })}>
    Einstellungen
  </button>
</nav>

<style>
  main {
    flex: 1;
    padding: max(16px, env(safe-area-inset-top)) var(--gasse) 24px;
    max-width: 640px;
    width: 100%;
    margin: 0 auto;
  }

  nav {
    position: sticky;
    bottom: 0;
    display: flex;
    gap: 2px;
    background: var(--flaeche);
    border-top: 1px solid var(--linie);
    padding: 6px var(--gasse) calc(6px + env(safe-area-inset-bottom));
  }

  nav button {
    flex: 1;
    padding: 9px 4px;
    font-size: 12px;
    color: var(--text-leise);
    border-radius: 8px;
  }

  nav button.aktiv {
    color: var(--text);
    background: var(--flaeche-hoch);
  }

  nav button.sync {
    color: var(--akzent);
  }

  .hinweis {
    background: var(--flaeche);
    border: 1px solid var(--akzent);
    border-radius: var(--radius);
    padding: 9px 12px;
    margin-bottom: 12px;
    font-size: 12px;
  }

  .hinweis.fehler {
    border-color: var(--kritisch);
    color: var(--kritisch);
  }

  .mitte {
    text-align: center;
    padding: 60px 0;
  }
</style>
