# Tory — Schnittstellendefinition

Der Vertrag zwischen Quellen, Kern und Oberflaeche. Ziel: **jede Quelle liefert
dieselben zwei Typen**, und der Startscreen braucht fuer keine eine
Sonderbehandlung.

Quelle der Wahrheit: [`crates/tory-core/src/model.rs`](../crates/tory-core/src/model.rs).
Diese Seite erklaert das Warum; die Datei ist verbindlich.

Die Bezeichner sind englisch, die Dokumentation deutsch. Die frueheren deutschen
Namen bilden sich so ab: `Signal` → `Signal`, `Dringlichkeit` → `Urgency`,
`QuellenId` → `SourceRef`, `DomaenenUebersicht` → `Overview`, `SyncStand` →
`SyncState`, `SyncFehler` → `SyncFault`.

## Die drei Ebenen

```
Quelle (Vault, Mindwtr, NocoDB, Gmail, Feed)
   │  Connector::fetch()      → holt und bildet ab
   ▼
Harvest { source, signals, overview }
   │  Store::apply_harvest()  → ersetzt die Zeilen dieser Quelle
   ▼
SQLite  ──  Engine::dashboard()  →  die Oberflaeche liest nur das
```

Die Oberflaeche sieht nie einen Connector und nie eine Antwort eines Dienstes.
Sie sieht `Signal`, `Overview` und `SyncState`.

## 1 · Signal — der gemeinsame Nenner

Alles, was auf den Startscreen darf, ist ein `Signal`.

| Feld | Wofuer |
| --- | --- |
| `id` | innerhalb der Quelle **stabil** — sonst verliert „spaeter bis" seinen Bezug |
| `source` | Art, Instanz und Beschriftung |
| `title`, `subtitle` | die zwei Zeilen jeder Signalzeile |
| `excerpt` | Vorschau auf Detailseiten. Nie der Volltext |
| `at` + `time_kind` | „heute faellig" vs. „in 5 h" vs. „vor 20 Min" |
| `window_end` | bei `Window`: die zweite Haelfte von „14–16 Uhr" |
| `urgency` | Farbe des Statusbalkens und Sortierung ueberall |
| `badge` | Projektname, Absender, Tabellenname |
| `tags` | fuer Filter auf den Detailseiten |
| `action` | was beim Antippen passiert |
| `completable` | darf in der Zeile abgehakt werden |
| `dedup_key` | dieselbe Sache aus zwei Quellen erscheint einmal |

### Warum `time_kind` ein eigenes Feld ist

Ein `DateTime` allein sagt nicht, wie es zu lesen ist. Dieselbe Zahl heisst bei
einer Aufgabe „faellig am", bei einem Termin „um", bei einer Mail „vor". Ohne
dieses Feld braeuchte die Oberflaeche pro Quelle eine Sonderregel — genau das,
was das Modell verhindern soll.

| `TimeKind` | Formulierung |
| --- | --- |
| `At` | „14:30", „morgen 09:00" |
| `Due` | „heute faellig", „Frist 3 T", „2 T ueberfaellig" |
| `Window` | „14–16 Uhr" (braucht `window_end`) |
| `Since` | „vor 20 Min" |

### Warum `dedup_key` nicht `id` ist

`id` identifiziert **den Datensatz in seiner Quelle**. `dedup_key` identifiziert
**die Sache in der Welt**. Eine Aufgabe, die in einem Vault und in Mindwtr
steht, hat zwei `id` und einen `dedup_key`. Der Store zeigt dann die erste —
nach Dringlichkeit sortiert, also die dringlichere.

## 2 · Overview — eine Struktur, drei Designrichtungen

Dieselben Daten bedienen die Karte in Richtung A, die Kennzahlenzeile in B und
die Kachel in C. Die Richtung entscheidet nur, welche Felder sie zeigt.

| Feld | A · Karte | B · Kennzahlenleiste | C · Kachel |
| --- | --- | --- | --- |
| `metric` / `caption` | Kartentitel-Zusatz | die Zahl in der Leiste | grosse Zahl |
| `note` | — | — | „1 mit Frist" |
| `lines` | Zeilen der Karte | — | Kachelinhalt |
| `progress` | — | Fortschrittsbalken | Balken |
| `metric_raw` | Schwellen | Sortierung | Farbwahl |

`metric` ist bereits formatiert (`"12"`, `"3.428,60 €"`), `metric_raw` derselbe
Wert als Zahl. Formatierung passiert einmal im Connector, nicht dreimal in drei
Designrichtungen.

## 3 · Connector — der Vertrag pro Quelle

```rust
#[async_trait]
pub trait Connector: Send + Sync {
    fn source(&self) -> &SourceRef;
    async fn fetch(&self, ctx: &SyncContext<'_>) -> Result<Harvest>;
    async fn complete(&self, ctx: &SyncContext<'_>, item_id: &str) -> Result<()>;
}
```

Regeln, die den Vertrag tragen:

* `fetch` wird **nur** vom Scheduler gerufen, nie aus der Oberflaeche.
* `complete` hat eine Vorgabe, die fehlschlaegt: nur lesend ist die Regel.
* Eine Quelle ohne Meldung liefert eine leere Signalliste — nicht `None`.
* `SyncContext` gibt Zugriff auf HTTP, Geheimnisse, `now` und den
  Zeitzonenversatz. Keine Datenbank, kein Weg in die Oberflaeche.

### Warum der Zeitzonenversatz durchgereicht wird

„Heute" ist lokal, nicht UTC. Ohne `tz_offset_minutes` kippte die Dringlichkeit
einer Aufgabe um Mitternacht UTC — in Berlin also um zwei Uhr morgens.

## 4 · Aktualitaet ist Teil der Daten

`SyncState` haengt an jeder Karte. Damit kann die App nie alte Zahlen als
aktuell ausgeben.

| `SyncFault` | Anzeige |
| --- | --- |
| `Offline` | stiller Graustich, kein Banner |
| `AuthExpired` | Banner mit Knopf |
| `RateLimited` | Hinweis mit `retry_after`, kein Wiederholungsversuch |
| `Misconfigured` | Hinweis; **nicht** im Takt wiederholt |
| `Server`, `Unknown` | Hinweis, Backoff |

`SyncFault::retryable()` steuert beides: ob der Scheduler es erneut versucht und
ob das Banner einen Knopf bekommt.

## 5 · Was der Kern nach aussen anbietet

`Engine` ist die einzige Tuer. Jeder Tauri-Befehl ist eine Zeile Weiterleitung.

| Methode | Zweck |
| --- | --- |
| `dashboard()` | alles fuer den Startscreen, aus der Datenbank, ohne Netz |
| `signals_of(source)` | die Detailseite |
| `sync(force)` | `false` = was faellig ist, `true` = alles |
| `act(action)` | fuehrt aus, was in Tory gehoert; gibt URLs zurueck |
| `snooze`, `dismiss` | die lokalen Marken |
| `config()`, `save_config()` | Speichern prueft und raeumt Geheimnisse auf |
| `set_secret()`, `secret_names()` | hinein ja, heraus nur die Namen |
| `begin_gmail_auth()`, `finish_gmail_auth()` | OAuth mit PKCE |
| `brief()` | das Tagesbriefing |

`save_config` gibt eine Liste von Problemen zurueck. Leer heisst uebernommen —
so bekommt die Oberflaeche ohne eigene Pruefregeln eine Rueckmeldung.

## Bewusst nicht im Modell

* **Kein Volltext.** Von Mails Betreff und `snippet`, von Notizen die ersten
  Zeilen. Fuer alles Weitere oeffnet Tory die Quelle.
* **Keine Schreiboperationen ausser einer.** Abhaken bei Mindwtr. Sonst nichts.
* **Kein Nutzerkonto, keine Geraete-Synchronisation.** Ein Telefon, eine
  Datenbank.
* **Kein Betrag, keine Waehrung.** Der fruehere Entwurf hatte ein Geld-Modul;
  Bankanbindung steht nicht mehr auf dem Plan, und ein Feld ohne Quelle ist
  Ballast.
