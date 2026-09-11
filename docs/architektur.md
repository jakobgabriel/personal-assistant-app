# Tory — Technische Architektur

## Stack

| Bereich | Wahl | Warum |
| --- | --- | --- |
| Rahmen | Tauri 2 | Eine Kiste Code fuer Android und Desktop; der Kern ist Rust, die Oberflaeche Web |
| Kern | Rust, eigene Kiste `tory-core` | Kein Tauri darin — dadurch ohne Android-SDK und ohne WebView testbar |
| Oberflaeche | Svelte 5 + TypeScript + Vite | Kleines Bundle (28 kB gzip), keine Laufzeit-Bibliothek im Auslieferungsstand |
| Persistenz | SQLite (`rusqlite`, gebuendelt) | Einzige Wahrheit auf dem Geraet; keine Systemabhaengigkeit |
| Netz | `reqwest` mit `rustls` | Kein OpenSSL, damit die Android-Uebersetzung ohne Systembibliotheken auskommt |
| Feeds | `feed-rs` | RSS 1/2, Atom und JSON Feed in einem Parser |
| WebDAV | `quick-xml` | PROPFIND-Antworten ereignisbasiert, ohne Namensraum-Annahmen |
| Geheimnisse | AES-256-GCM, Schluessel im privaten App-Verzeichnis | Siehe *Geheimnisse* unten — mit einer klaren Aussage darueber, was das nicht leistet |
| Anmeldung | OAuth 2 mit PKCE, ohne Client-Secret | Eine App auf dem Telefon kann kein Geheimnis huegten |
| Rueckkanal | `tauri-plugin-deep-link` | `de.tory.app://oauth2` erreicht die App, auch wenn sie gerade startet |

Ziel-SDK Android 24+ (Android 7). Desktop laeuft auf allem, was Tauri 2 traegt.

## Aufbau

```
crates/tory-core/          Rust, kein Tauri
  model.rs                 Signal, Urgency, SourceRef, Overview, SyncState
  config.rs                Was der Nutzer einstellt (ohne Geheimnisse)
  secrets.rs               Verschluesselter Ablageort fuer Tokens
  store.rs                 SQLite: Signale, Uebersichten, Sync-Stand, lokale Marken
  sync.rs                  Wer ist faellig? Takt und Backoff
  engine.rs                Die eine Tuer, durch die die Oberflaeche geht
  http.rs                  Ein Client, eine Fehlerbehandlung
  markdown.rs              Frontmatter und offene Checkboxen
  connectors/              obsidian, mindwtr, nocodb, gmail, feeds
  ai/                      Provider-Vertrag, Anthropic, OpenAI, Ollama

app/                       Tauri-App
  src/                     Svelte-Oberflaeche
  src-tauri/src/           Schale: Fenster, Zustand, Befehle
```

Die Trennung ist der Kern der Sache: `app/src-tauri` enthaelt keine
Entscheidung. Jeder Befehl ist eine Zeile Weiterleitung an `Engine`. Was
entschieden wird, liegt in `tory-core` — und ist damit ohne Emulator, ohne
WebView und ohne Netz pruefbar.

```
$ cargo test -p tory-core
test result: ok. 106 passed
```

## Der Vertrag pro Quelle

```rust
#[async_trait]
pub trait Connector: Send + Sync {
    fn source(&self) -> &SourceRef;
    async fn fetch(&self, ctx: &SyncContext<'_>) -> Result<Harvest>;
    async fn complete(&self, ctx: &SyncContext<'_>, item_id: &str) -> Result<()> { … }
}
```

`Harvest` ist `{ source, signals, overview }`. Eine Quelle anzubinden heisst:
`Connector` implementieren und in `sync::build_connectors` registrieren. Der
Startscreen aendert sich dabei nicht — er kennt nur `Signal` und `Overview`.

`complete` hat eine Vorgabe, die fehlschlaegt: die meisten Quellen sind nur
lesend, und das soll die Regel bleiben, nicht die Ausnahme.

### Zweigeteilt, damit es pruefbar ist

Jeder Connector besteht aus zwei Haelften:

```
fetch()          HTTP/Dateisystem  →  rohe Antwort
map_…()          rohe Antwort      →  Harvest        ← rein, ohne Netz
```

In `crates/tory-core/tests/fixtures/` liegen echte Antworten der Dienste. Die
Abbildung ist gegen sie festgenagelt — das ist der Teil, in dem die Fehler
stecken (falsches Datumsformat, verschachteltes Feld, HTML im Text), und es ist
der Teil, der ohne laufenden Server prueffbar ist.

## Ereignismodell

```rust
pub struct Signal {
    pub id: String,
    pub source: SourceRef,
    pub title: String,
    pub subtitle: Option<String>,
    pub excerpt: Option<String>,
    pub at: Option<DateTime<Utc>>,
    pub time_kind: Option<TimeKind>,   // At | Due | Window | Since
    pub window_end: Option<DateTime<Utc>>,
    pub urgency: Urgency,              // Critical | High | Normal | Info
    pub badge: Option<String>,
    pub tags: Vec<String>,
    pub action: Option<Action>,
    pub completable: bool,
    pub dedup_key: Option<String>,
}
```

Sortierung: `urgency`, dann Naehe von `at`. Nichts anderes.

`time_kind` ist der Grund, warum die Oberflaeche aus einem Zeitstempel die
richtige Formulierung ableiten kann — „heute faellig" gegen „in 5 h" gegen „vor
20 Min" — ohne pro Domaene eine Sonderregel zu kennen.

`dedup_key` faltet dieselbe Sache aus zwei Quellen zusammen: eine Aufgabe, die
in Obsidian und in Mindwtr steht, ist eine Zeile. Dieselbe Meldung aus zwei
Feeds ebenso.

## Speicher

SQLite ist die einzige Wahrheit auf dem Geraet. Die Oberflaeche liest **nie**
direkt vom Netz. Daraus folgt dreierlei: die App ist beim Start sofort da, ein
Hintergrund-Sync tauscht Zeilen aus, und der Ausfall einer Quelle laesst die
anderen Karten unberuehrt.

| Tabelle | Inhalt |
| --- | --- |
| `signals` | pro Quelle **ersetzt**, nicht gemergt |
| `overviews` | eine Zeile je Quelle |
| `sync_state` | `last_ok`, `last_attempt`, `fault`, `failures` |
| `local_marks` | „spaeter bis" und „erledigt" — ueberlebt das Ersetzen |

Das Ersetzen ist Absicht: eine Quelle ist die Wahrheit ueber ihre eigenen
Signale, und was sie nicht mehr liefert, ist weg. Deshalb liegen die lokalen
Zusaetze in einer eigenen Tabelle — sonst waere eine weggewischte Zeile nach dem
naechsten Sync wieder da.

## Takt und Backoff

Kein langlebiger Hintergrunddienst: unter Android stirbt er ohnehin. Stattdessen
fragt die App beim Oeffnen und danach in Abstaenden, *was faellig ist*.

| Zustand | Wartezeit bis zum naechsten Versuch |
| --- | --- |
| noch nie gelaufen | sofort |
| zuletzt erfolgreich | der konfigurierte Takt |
| `n` Fehlversuche | Takt × 2ⁿ, gedeckelt bei 2 h |
| `Misconfigured` / `AuthExpired` | 1 h — das heilt nicht durch Warten |

Der letzte Fall ist der wichtige: eine falsche Tabellen-Id wird nicht alle 15
Minuten neu ausprobiert. Dafuer gibt es den Knopf in den Einstellungen.

## Fehler, die unterschiedlich aussehen muessen

`SyncFault` trennt die Faelle, weil die Oberflaeche sie verschieden zeigt:

| Variante | Anzeige |
| --- | --- |
| `Offline` | stiller Graustich, kein Banner |
| `AuthExpired` | Banner mit Knopf — der Gmail-Fall |
| `RateLimited` | Hinweis mit `retry_after`, kein Wiederholungsversuch |
| `Misconfigured` | Hinweis in den Einstellungen |
| `Server`, `Unknown` | Hinweis, Backoff |

`SyncState` haengt an jeder Karte. Damit kann die App nie alte Zahlen als
aktuell ausgeben; aelter als zwei Stunden wird die Karte sichtbar blass.

## Geheimnisse

Zwei Dateien, streng getrennt:

* `config.json` — URLs, Vault-Namen, Feed-Adressen, Takte. Lesbar, exportierbar,
  versionierbar. Enthaelt nie ein Geheimnis, sondern nur den *Schluesselnamen*.
* `secrets/secrets.bin` — mit AES-256-GCM verschluesselt, Schluessel in
  `secrets/device.key`, beide mit Dateirechten 0600 und atomar geschrieben.

Auf Android liegt das private Verzeichnis in der App-Sandbox und ist ohne Root
fuer andere Apps nicht lesbar.

**Was das nicht ist:** hardwaregestuetzte Schluesselhaltung. Der Geraeteschluessel
liegt neben den Daten. Gegen jemanden mit Dateizugriff — entsperrtes Telefon,
Root, Geraeteabbild — hilft das nicht. Dafuer braucht es den Android Keystore;
das steht in `roadmap-ki.md` unter „spaeter" und ist bewusst nicht als erledigt
ausgegeben.

`Config::referenced_secret_keys()` sagt, welche Eintraege noch gebraucht werden.
Beim Speichern raeumt die Engine den Rest weg — eine entfernte Quelle
hinterlaesst kein Token.

## Gmail-Anmeldung

```
App  ──(1) Systembrowser: accounts.google.com + code_challenge
     ←─(2) de.tory.app://oauth2?code=…&state=…     (Deep Link)
     ──(3) POST oauth2.googleapis.com/token + code_verifier
     ←─(4) refresh_token  →  Secret-Store
```

Ausdruecklich im Systembrowser, nicht im WebView: ein eingebettetes
Anmeldefenster ist genau das Muster, vor dem Google warnt. `state` wird gegen
den laufenden Anmeldeversuch geprueft — eine untergeschobene Rueckleitung wird
abgewiesen.

Access-Tokens leben eine Stunde. Tory speichert sie nicht, sondern holt bei
jedem Sync ein frisches: das kostet einen Aufruf und spart die gesamte
Ablauflogik.

## Berechtigungen im WebView

```json
"permissions": ["core:default", "opener:allow-open-url", "deep-link:default", "log:default"]
```

Mehr braucht die Oberflaeche nicht. Kein `http`, kein `fs`, kein `shell`: jeder
Netzzugriff und jeder Dateizugriff passiert in Rust. Ein Fehler in der
Oberflaeche kann damit nicht zu einem Zugriff auf das Dateisystem werden.

## Was bewusst fehlt

* **Kein eigenes Backend.** Alle Dienste sind bereits selbst gehostet; ein
  weiterer Dienst dazwischen waere eine weitere Sache, die ausfaellt.
* **Kein Volltext.** Von Mails nur Betreff, Absender und Googles `snippet`; von
  Notizen die ersten Zeilen. Fuer alles Weitere oeffnet Tory die Quelle.
* **Kaum Schreibzugriffe.** Genau einer geht nach aussen: eine Mindwtr-Aufgabe
  abhaken. Alles andere ist lesend.
* **Keine Geraete-Synchronisation.** Ein Telefon, eine Datenbank.
