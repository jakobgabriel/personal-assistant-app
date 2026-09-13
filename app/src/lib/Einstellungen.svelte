<script lang="ts">
  // Alles, was der Nutzer konfigurieren kann — inklusive Anlegen und Entfernen
  // von Quellen. Gearbeitet wird auf einer Kopie; erst "Speichern" schreibt.
  import * as api from "./api";
  import Feld from "./Feld.svelte";
  import Geheimnis from "./Geheimnis.svelte";
  import { app } from "./state.svelte";
  import type {
    Config,
    Feed,
    FeedsSource,
    GmailSource,
    MindwtrSource,
    NocodbSource,
    NocodbTableMap,
    ObsidianSource,
  } from "./types";

  // `$state.snapshot` loest die Proxys auf — sonst landen sie in `invoke`.
  let entwurf = $state<Config>(structuredClone($state.snapshot(app.config!)));
  let probleme = $state<string[]>([]);
  let gespeichert = $state(false);
  let offen = $state<string | null>("quellen");

  const abschnitt = (name: string) => (offen = offen === name ? null : name);

  function gemeinsam(instance: string, label: string, minuten: number) {
    return { instance, label, enabled: true, cadence: { minutes: minuten }, order: 0 };
  }

  async function speichern() {
    probleme = await app.saveConfig(structuredClone($state.snapshot(entwurf)));
    if (probleme.length === 0) {
      gespeichert = true;
      setTimeout(() => (gespeichert = false), 2000);
    }
  }

  function neuerVault() {
    const n = entwurf.obsidian.length + 1;
    const instance = `vault${n}`;
    entwurf.obsidian.push({
      ...gemeinsam(instance, `Vault ${n}`, 30),
      vault_name: "",
      access: { kind: "local", path: "" },
      include_folders: [],
      exclude_folders: [".obsidian", ".trash", "Archiv", "Templates"],
      read_tasks: true,
      pinned_tags: [],
      scan_limit: 400,
    } satisfies ObsidianSource);
  }

  function neuesMindwtr() {
    const instance = `mindwtr${entwurf.mindwtr.length + 1}`;
    entwurf.mindwtr.push({
      ...gemeinsam(instance, "Mindwtr", 15),
      base_url: "https://",
      token_key: `mindwtr.${instance}.token`,
      statuses: ["inbox", "next", "waiting"],
      include_undated: false,
      horizon_days: 7,
    } satisfies MindwtrSource);
  }

  function neuesNocodb() {
    const instance = `noco${entwurf.nocodb.length + 1}`;
    entwurf.nocodb.push({
      ...gemeinsam(instance, "NocoDB", 60),
      base_url: "https://",
      token_key: `nocodb.${instance}.token`,
      tables: [],
    } satisfies NocodbSource);
  }

  function neueTabelle(quelle: NocodbSource) {
    quelle.tables.push({
      table_id: "",
      label: "Tabelle",
      title_field: "Title",
      done_values: [],
      limit: 100,
    } satisfies NocodbTableMap);
  }

  function neuesGmail() {
    const instance = `gmail${entwurf.gmail.length + 1}`;
    entwurf.gmail.push({
      ...gemeinsam(instance, "Gmail", 15),
      client_id: "",
      redirect_uri: "de.tory.app://oauth2",
      queries: [
        { label: "Wichtig & ungelesen", query: "is:unread is:important newer_than:14d", urgency: "high" },
      ],
      per_query_limit: 15,
    } satisfies GmailSource);
  }

  function neueFeeds() {
    entwurf.feeds.push({
      ...gemeinsam("nachrichten", "Nachrichten", 30),
      feeds: [],
      headline_limit: 10,
      max_age_hours: 36,
    } satisfies FeedsSource);
  }

  function neuerFeed(quelle: FeedsSource) {
    quelle.feeds.push({ url: "https://", label: "Feed", topic: "global", enabled: true } satisfies Feed);
  }

  /** Kommagetrennte Eingabe <-> Liste. */
  const alsListe = (raw: string): string[] =>
    raw.split(",").map((t) => t.trim()).filter(Boolean);

  async function anmelden(instance: string) {
    try {
      await api.beginGmailAuth(instance);
    } catch (e) {
      probleme = [String(e)];
    }
  }
</script>

<header><h1>Einstellungen</h1></header>

{#if probleme.length > 0}
  <div class="probleme">
    {#each probleme as p (p)}<div>{p}</div>{/each}
  </div>
{/if}

<!-- Startscreen -->
<section>
  <button class="kopf" onclick={() => abschnitt("start")}>
    <span>Startscreen</span><span class="pfeil">{offen === "start" ? "−" : "+"}</span>
  </button>
  {#if offen === "start"}
    <div class="inhalt fade">
      <Feld label="Signale oben" bind:value={entwurf.dashboard.top_signals} typ="number"
        hinweis="Wie viele Zeilen unter 'Jetzt wichtig' stehen." />
      <label class="feld">
        <span class="label">Darstellung</span>
        <select bind:value={entwurf.dashboard.theme}>
          <option value="cockpit">Cockpit — dunkel, dicht</option>
          <option value="calm">Ruhiger Morgen — hell, luftig</option>
        </select>
      </label>
      <Feld label="Morgenbrief um" bind:value={
        () => entwurf.dashboard.morning_brief_at ?? "",
        (v: string | number) => (entwurf.dashboard.morning_brief_at = String(v) || undefined)
      } platzhalter="07:30" hinweis="Leer lassen schaltet ihn ab." />
    </div>
  {/if}
</section>

<!-- Quellen -->
<section>
  <button class="kopf" onclick={() => abschnitt("quellen")}>
    <span>Quellen</span><span class="pfeil">{offen === "quellen" ? "−" : "+"}</span>
  </button>
  {#if offen === "quellen"}
    <div class="inhalt fade">

      <h3>Obsidian</h3>
      {#each entwurf.obsidian as vault, i (i)}
        <div class="block">
          <div class="blockkopf">
            <input class="titel" bind:value={vault.label} aria-label="Bezeichnung" />
            <button class="weg" onclick={() => entwurf.obsidian.splice(i, 1)}>entfernen</button>
          </div>
          <Feld label="Kennung" bind:value={vault.instance}
            hinweis="Eindeutig, taucht als obsidian:… in der Datenbank auf." />
          <Feld label="Vault-Name in Obsidian" bind:value={vault.vault_name}
            hinweis="Muss exakt stimmen, sonst funktioniert der Sprung in die App nicht." />
          <label class="feld">
            <span class="label">Zugriff</span>
            <select value={vault.access.kind} onchange={(e) => {
              const k = (e.currentTarget as HTMLSelectElement).value;
              vault.access = k === "local"
                ? { kind: "local", path: "" }
                : { kind: "webdav", base_url: "https://", username: "", password_key: `obsidian.${vault.instance}.webdav` };
            }}>
              <option value="local">Ordner auf dem Geraet</option>
              <option value="webdav">WebDAV (Nextcloud, rclone)</option>
            </select>
          </label>
          {#if vault.access.kind === "local"}
            <Feld label="Pfad" bind:value={vault.access.path} platzhalter="/home/jakob/Obsidian/Privat" />
          {:else}
            <Feld label="WebDAV-Basis-URL" bind:value={vault.access.base_url} typ="url"
              platzhalter="https://cloud.example.de/remote.php/dav/files/jakob/Obsidian/Privat" />
            <Feld label="Benutzername" bind:value={vault.access.username} />
            <Geheimnis name={vault.access.password_key} label="WebDAV-Passwort" />
          {/if}
          <Feld label="Angepinnte Tags" bind:value={
            () => vault.pinned_tags.join(", "),
            (v: string | number) => (vault.pinned_tags = alsListe(String(v)))
          } platzhalter="merker, wichtig" hinweis="Notizen mit diesen Tags erscheinen immer." />
          <Feld label="Ordner ausschliessen" bind:value={
            () => vault.exclude_folders.join(", "),
            (v: string | number) => (vault.exclude_folders = alsListe(String(v)))
          } />
          <Feld label="Takt (Minuten)" bind:value={vault.cadence.minutes} typ="number" />
          <label class="schalter">
            <input type="checkbox" bind:checked={vault.read_tasks} />
            <span>Offene Checkboxen als Aufgaben lesen</span>
          </label>
          <label class="schalter">
            <input type="checkbox" bind:checked={vault.enabled} />
            <span>Aktiv</span>
          </label>
        </div>
      {/each}
      <button class="hinzu" onclick={neuerVault}>+ Vault</button>

      <h3>Mindwtr</h3>
      {#each entwurf.mindwtr as m, i (i)}
        <div class="block">
          <div class="blockkopf">
            <input class="titel" bind:value={m.label} aria-label="Bezeichnung" />
            <button class="weg" onclick={() => entwurf.mindwtr.splice(i, 1)}>entfernen</button>
          </div>
          <Feld label="Kennung" bind:value={m.instance} />
          <Feld label="Basis-URL der Cloud" bind:value={m.base_url} typ="url"
            platzhalter="https://mindwtr.example.de" hinweis="Ohne /v1 — das haengt Tory selbst an." />
          <Geheimnis name={m.token_key} label="Bearer-Token" />
          <Feld label="Status" bind:value={
            () => m.statuses.join(", "),
            (v: string | number) => (m.statuses = alsListe(String(v)))
          } hinweis="inbox, next, waiting, someday, reference" />
          <Feld label="Horizont (Tage)" bind:value={m.horizon_days} typ="number"
            hinweis="Ab wann eine Faelligkeit auf den Startscreen darf." />
          <Feld label="Takt (Minuten)" bind:value={m.cadence.minutes} typ="number" />
          <label class="schalter">
            <input type="checkbox" bind:checked={m.include_undated} />
            <span>Auch Aufgaben ohne Datum zeigen</span>
          </label>
          <label class="schalter">
            <input type="checkbox" bind:checked={m.enabled} />
            <span>Aktiv</span>
          </label>
        </div>
      {/each}
      <button class="hinzu" onclick={neuesMindwtr}>+ Mindwtr</button>

      <h3>NocoDB</h3>
      {#each entwurf.nocodb as n, i (i)}
        <div class="block">
          <div class="blockkopf">
            <input class="titel" bind:value={n.label} aria-label="Bezeichnung" />
            <button class="weg" onclick={() => entwurf.nocodb.splice(i, 1)}>entfernen</button>
          </div>
          <Feld label="Kennung" bind:value={n.instance} />
          <Feld label="Basis-URL" bind:value={n.base_url} typ="url" platzhalter="https://noco.example.de"
            hinweis="Ohne /api — das haengt Tory selbst an." />
          <Geheimnis name={n.token_key} label="API-Token (xc-token)" />
          {#each n.tables as t, ti (ti)}
            <div class="untertabelle">
              <div class="blockkopf">
                <input class="titel" bind:value={t.label} aria-label="Tabellenname" />
                <button class="weg" onclick={() => n.tables.splice(ti, 1)}>entfernen</button>
              </div>
              <Feld label="Tabellen-Id" bind:value={t.table_id} platzhalter="mtbl…" />
              <Feld label="Titelspalte" bind:value={t.title_field} />
              <Feld label="Datumsspalte" bind:value={
                () => t.date_field ?? "",
                (v: string | number) => (t.date_field = String(v) || undefined)
              } hinweis="Bestimmt die Dringlichkeit. Leer = alles nur Info." />
              <Feld label="Statusspalte" bind:value={
                () => t.status_field ?? "",
                (v: string | number) => (t.status_field = String(v) || undefined)
              } />
              <Feld label="Erledigt-Werte" bind:value={
                () => t.done_values.join(", "),
                (v: string | number) => (t.done_values = alsListe(String(v)))
              } hinweis="Diese Zeilen erzeugen kein Signal mehr." />
              <Feld label="Filter" bind:value={
                () => t.filter ?? "",
                (v: string | number) => (t.filter = String(v) || undefined)
              } platzhalter="(Status,neq,Abgelehnt)" />
            </div>
          {/each}
          <button class="hinzu klein" onclick={() => neueTabelle(n)}>+ Tabelle</button>
          <Feld label="Takt (Minuten)" bind:value={n.cadence.minutes} typ="number" />
          <label class="schalter">
            <input type="checkbox" bind:checked={n.enabled} />
            <span>Aktiv</span>
          </label>
        </div>
      {/each}
      <button class="hinzu" onclick={neuesNocodb}>+ NocoDB</button>

      <h3>Gmail</h3>
      {#each entwurf.gmail as g, i (i)}
        <div class="block">
          <div class="blockkopf">
            <input class="titel" bind:value={g.label} aria-label="Bezeichnung" />
            <button class="weg" onclick={() => entwurf.gmail.splice(i, 1)}>entfernen</button>
          </div>
          <Feld label="Kennung" bind:value={g.instance} />
          <Feld label="OAuth-Client-Id" bind:value={g.client_id}
            platzhalter="…apps.googleusercontent.com"
            hinweis="Eigenes Google-Cloud-Projekt, Typ 'Desktop' oder 'Android'." />
          <Feld label="Rueckleitung" bind:value={g.redirect_uri} />
          {#each g.queries as q, qi (qi)}
            <div class="untertabelle">
              <div class="blockkopf">
                <input class="titel" bind:value={q.label} aria-label="Gruppenname" />
                <button class="weg" onclick={() => g.queries.splice(qi, 1)}>entfernen</button>
              </div>
              <Feld label="Suche" bind:value={q.query} platzhalter="is:unread is:important newer_than:14d" />
              <label class="feld">
                <span class="label">Dringlichkeit</span>
                <select bind:value={q.urgency}>
                  <option value="critical">Jetzt</option>
                  <option value="high">Heute</option>
                  <option value="normal">Bald</option>
                  <option value="info">Info</option>
                </select>
              </label>
            </div>
          {/each}
          <button class="hinzu klein"
            onclick={() => g.queries.push({ label: "Neue Suche", query: "is:unread", urgency: "normal" })}>
            + Suche
          </button>
          <button class="hinzu" onclick={() => anmelden(g.instance)}>
            {app.hatSecret(`gmail.${g.instance}.refresh_token`) ? "Neu anmelden" : "Bei Google anmelden"}
          </button>
          <Feld label="Takt (Minuten)" bind:value={g.cadence.minutes} typ="number" />
          <label class="schalter">
            <input type="checkbox" bind:checked={g.enabled} />
            <span>Aktiv</span>
          </label>
        </div>
      {/each}
      <button class="hinzu" onclick={neuesGmail}>+ Gmail</button>

      <h3>Nachrichten</h3>
      {#each entwurf.feeds as f, i (i)}
        <div class="block">
          <div class="blockkopf">
            <input class="titel" bind:value={f.label} aria-label="Bezeichnung" />
            <button class="weg" onclick={() => entwurf.feeds.splice(i, 1)}>entfernen</button>
          </div>
          {#each f.feeds as feed, fi (fi)}
            <div class="untertabelle">
              <div class="blockkopf">
                <input class="titel" bind:value={feed.label} aria-label="Feedname" />
                <button class="weg" onclick={() => f.feeds.splice(fi, 1)}>entfernen</button>
              </div>
              <Feld label="Adresse" bind:value={feed.url} typ="url" />
              <label class="feld">
                <span class="label">Art</span>
                <select bind:value={feed.topic}>
                  <option value="local">Regional</option>
                  <option value="global">Welt</option>
                  <option value="weather">Wetter</option>
                </select>
              </label>
            </div>
          {/each}
          <button class="hinzu klein" onclick={() => neuerFeed(f)}>+ Feed</button>
          <Feld label="Schlagzeilen auf dem Startscreen" bind:value={f.headline_limit} typ="number" />
          <Feld label="Hoechstalter (Stunden)" bind:value={f.max_age_hours} typ="number" />
          <Feld label="Takt (Minuten)" bind:value={f.cadence.minutes} typ="number" />
          <label class="schalter">
            <input type="checkbox" bind:checked={f.enabled} />
            <span>Aktiv</span>
          </label>
        </div>
      {/each}
      {#if entwurf.feeds.length === 0}
        <button class="hinzu" onclick={neueFeeds}>+ Nachrichtenquelle</button>
      {/if}
    </div>
  {/if}
</section>

<!-- AI -->
<section>
  <button class="kopf" onclick={() => abschnitt("ki")}>
    <span>AI</span><span class="pfeil">{offen === "ki" ? "−" : "+"}</span>
  </button>
  {#if offen === "ki"}
    <div class="inhalt fade">
      <p class="leise erklaerung">
        Erster Schritt der Roadmap: ein Tagesbriefing aus den Signalen, die ohnehin
        schon sortiert vorliegen. Was das Modell sieht, steuert der Schalter unten.
      </p>
      <label class="schalter">
        <input type="checkbox" bind:checked={entwurf.ai.enabled} />
        <span>AI einschalten</span>
      </label>
      <label class="feld">
        <span class="label">Anbieter</span>
        <select bind:value={entwurf.ai.provider}>
          <option value="anthropic">Anthropic</option>
          <option value="open_ai">OpenAI</option>
          <option value="ollama">Ollama (eigenes Netz)</option>
        </select>
      </label>
      <Feld label="Modell" bind:value={entwurf.ai.model} platzhalter="claude-sonnet-5" />
      {#if entwurf.ai.provider === "ollama"}
        <Feld label="Basis-URL" bind:value={
          () => entwurf.ai.base_url ?? "",
          (v: string | number) => (entwurf.ai.base_url = String(v) || undefined)
        } platzhalter="http://192.168.1.20:11434" />
      {:else}
        <Geheimnis
          name={entwurf.ai.provider === "anthropic" ? "ai.anthropic.key" : "ai.openai.key"}
          label="API-Schluessel" />
        <button class="hinzu klein" onclick={() => (entwurf.ai.api_key_key =
          entwurf.ai.provider === "anthropic" ? "ai.anthropic.key" : "ai.openai.key")}>
          Schluessel dieser App zuordnen
        </button>
      {/if}
      <label class="schalter">
        <input type="checkbox" bind:checked={entwurf.ai.titles_only} />
        <span>Nur Titel und Zeiten senden, keine Inhalte</span>
      </label>
    </div>
  {/if}
</section>

<!-- Ueber -->
<section>
  <button class="kopf" onclick={() => abschnitt("ueber")}>
    <span>Ueber</span><span class="pfeil">{offen === "ueber" ? "−" : "+"}</span>
  </button>
  {#if offen === "ueber" && app.appInfo}
    <div class="inhalt fade">
      <p class="leise erklaerung">
        Version {app.appInfo.version} auf {app.appInfo.platform}.<br />
        Daten liegen in <code>{app.appInfo.data_dir}</code>. Geheimnisse liegen dort
        verschluesselt in <code>secrets/</code> und verlassen das Geraet nur auf dem
        Weg zu dem Dienst, zu dem sie gehoeren.
      </p>
    </div>
  {/if}
</section>

<div class="fusszeile">
  <button class="primaer" onclick={speichern}>
    {gespeichert ? "gespeichert" : "Speichern"}
  </button>
</div>

<style>
  header h1 {
    font-size: 20px;
    font-weight: 600;
    margin: 4px 0 16px;
  }

  section {
    border-bottom: 1px solid var(--linie);
  }

  .kopf {
    display: flex;
    justify-content: space-between;
    width: 100%;
    padding: 13px 0;
    font-size: 15px;
  }

  .pfeil {
    color: var(--text-sehr-leise);
  }

  .inhalt {
    padding-bottom: 16px;
  }

  h3 {
    font-size: 11px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-sehr-leise);
    margin: 18px 0 8px;
  }

  .block,
  .untertabelle {
    border: 1px solid var(--linie);
    border-radius: var(--radius);
    padding: 12px;
    margin-bottom: 10px;
    background: var(--flaeche);
  }

  .untertabelle {
    background: var(--grund);
    margin-top: 10px;
  }

  .blockkopf {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 10px;
  }

  .titel {
    font-weight: 600;
    border: none;
    background: none;
    padding: 0;
  }

  .weg {
    flex: 0 0 auto;
    font-size: 11px;
    color: var(--kritisch);
  }

  .hinzu {
    display: block;
    width: 100%;
    border: 1px dashed var(--linie);
    border-radius: var(--radius);
    padding: 9px;
    font-size: 13px;
    color: var(--text-leise);
    margin-bottom: 10px;
  }

  .hinzu.klein {
    padding: 6px;
    font-size: 12px;
  }

  .schalter {
    display: flex;
    gap: 9px;
    align-items: center;
    font-size: 13px;
    margin-bottom: 10px;
  }

  .schalter input {
    /* 18 px statt der Voreinstellung: mit dem Finger ist das der Unterschied
       zwischen treffen und danebentippen. */
    width: 18px;
    height: 18px;
    flex: 0 0 auto;
    accent-color: var(--akzent);
  }

  .feld {
    display: block;
    margin-bottom: 12px;
  }

  .feld select {
    margin-top: 4px;
  }

  .erklaerung {
    font-size: 12px;
    margin: 0 0 12px;
  }

  code {
    font-family: var(--mono);
    font-size: 11px;
    word-break: break-all;
  }

  .probleme {
    background: var(--flaeche);
    border: 1px solid var(--kritisch);
    border-radius: var(--radius);
    padding: 10px 12px;
    margin-bottom: 14px;
    font-size: 12px;
    color: var(--kritisch);
  }

  .fusszeile {
    padding: 18px 0 8px;
  }

  .primaer {
    width: 100%;
    background: var(--akzent);
    color: var(--grund);
    border-radius: 999px;
    padding: 11px;
    font-weight: 600;
  }
</style>
