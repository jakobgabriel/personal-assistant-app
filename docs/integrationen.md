# Tory — Quellen einrichten

Was Tory pro Quelle liest, welcher Endpunkt dahintersteckt und was du dafuer
brauchst. Alles ist in der App einstellbar; diese Seite erklaert, was in die
Felder gehoert.

**Geheimnisse werden nie in `config.json` geschrieben.** Dort steht nur der
*Schluesselname* (etwa `nocodb.haupt.token`); der Wert liegt verschluesselt in
`secrets/`. Deshalb kannst du deine Konfiguration weitergeben, ohne aufzuraeumen.

---

## 1 · Obsidian — mehrere Vaults

Obsidian selbst hat keine Schnittstelle: ein Vault ist ein Ordner mit Markdown.
Tory kommt auf zwei Wegen an die Dateien.

### Weg A: Ordner auf dem Geraet

Auf dem Desktop der Normalfall. Auf dem Telefon nur sinnvoll, wenn ein Ordner
lokal gespiegelt wird (Syncthing, FolderSync).

| Feld | Beispiel |
| --- | --- |
| Pfad | `/home/jakob/Obsidian/Privat` |

### Weg B: WebDAV

Der Weg, der vom Telefon aus zuverlaessig funktioniert. Nextcloud, `rclone serve
webdav`, jeder WebDAV-Server.

| Feld | Beispiel |
| --- | --- |
| Basis-URL | `https://cloud.example.de/remote.php/dav/files/jakob/Obsidian/Privat` |
| Benutzername | `jakob` |
| Passwort | bei Nextcloud ein **App-Passwort**, nicht das Kontopasswort |

Tory macht ein `PROPFIND` mit `Depth: 1` je Ordner und ein `GET` je
Markdown-Datei. `Depth: infinity` waere ein Aufruf statt vieler, aber Nextcloud
und andere verbieten es.

### Was gelesen wird

* **Offene Checkboxen** — `- [ ] …`, `* [ ] …`, `1. [ ] …`. Abgehaktes nie.
* **Faelligkeiten** in beiden gebraeuchlichen Schreibweisen:
  * Tasks-Plugin: `📅 2026-09-30` (faellig), `⏳`/`🛫` (geplant), `⏫`/`🔼` (hoch)
  * Dataview: `due:: 2026-09-30`, `scheduled:: 2026-09-25`
* **Frontmatter** — `title:` und `tags:`, als Liste oder in eckigen Klammern.
* **Angepinnte Tags** — Notizen mit einem dieser Tags erscheinen immer als
  Signal, unabhaengig von Aufgaben.

Eine Aufgabe **ohne Datum** wird gezaehlt, aber nicht zum Signal — sonst flutet
ein grosser Vault den Startscreen. Ausnahme: `⏫` markierte kommen durch.

Antippen oeffnet `obsidian://open?vault=…&file=…`. Dafuer muss **Vault-Name**
exakt so heissen wie in Obsidian.

`.obsidian`, `.trash` und alles mit fuehrendem Punkt werden nie betreten.
`scan_limit` (Vorgabe 400 Dateien) deckelt einen Sync — WebDAV ist langsam.

---

## 2 · Mindwtr — selbst gehostet

Angebunden wird der **`mindwtr-cloud`-Dienst** aus dem Docker-Stack, nicht die
lokale API der Desktop-App: die bindet auf `127.0.0.1` und ist vom Telefon aus
nicht erreichbar.

### Server

```dotenv
# .env neben der compose.yaml
MINDWTR_CLOUD_AUTH_TOKENS=ein_langes_zufaelliges_token_mindestens_20_zeichen
```

Die REST-Schnittstelle liegt dann unter `http://HOST:8787/v1`.

| Feld in Tory | Wert |
| --- | --- |
| Basis-URL | `https://mindwtr.example.de` — **ohne** `/v1`, das haengt Tory an |
| Bearer-Token | derselbe Wert wie in `MINDWTR_CLOUD_AUTH_TOKENS` |

### Endpunkte

| Zweck | Aufruf |
| --- | --- |
| Aufgaben | `GET /v1/tasks?status=next&limit=200` |
| Projekte | `GET /v1/projects?limit=200` |
| Abhaken | `POST /v1/tasks/{id}/complete` |

Pro konfiguriertem Status ein Aufruf — `status` nimmt nur einen Wert.

### Einstellungen

* **Status** — `inbox`, `next`, `waiting`, `someday`, `reference`. Vorgabe:
  die ersten drei.
* **Horizont** — wie viele Tage im Voraus eine Faelligkeit interessant wird
  (Vorgabe 7). Was weiter weg liegt, bleibt in Mindwtr.
* **Auch ohne Datum** — aus, ausser du willst den Eingang auf dem Startscreen.

Als Tagesfokus markierte Aufgaben (`isFocusedToday`) kommen immer durch, auch
ohne Datum. Projektnamen werden als Beschriftung gezogen; faellt der Aufruf aus,
laeuft der Sync ohne sie weiter.

**Abhaken** ist der einzige schreibende Aufruf, den Tory nach aussen macht — und
er passiert in dieser Reihenfolge: erst der Server, dann lokal. Andersherum saehe
die Aufgabe kurz erledigt aus und waere beim naechsten Sync wieder da.

---

## 3 · NocoDB — beliebige Tabellen

NocoDB hat keine feste Bedeutung fuer seine Spalten: eine Tabelle kann
Bewerbungen, Wartungstermine oder Vertraege enthalten. Deshalb bringt jede
angebundene Tabelle eine **Zuordnung** mit.

### Token

In NocoDB: Konto → *Tokens* → neues Token. Es geht als `xc-token`-Kopfzeile mit.

| Feld in Tory | Wert |
| --- | --- |
| Basis-URL | `https://noco.example.de` — **ohne** `/api` |
| API-Token | der Tokenwert |

### Tabellen-Id finden

Sie steht in der URL, wenn du die Tabelle offen hast, und faengt mit `m` an.
Oder ueber die Meta-API:

```bash
curl -H "xc-token: DEIN_TOKEN" https://noco.example.de/api/v2/meta/bases
curl -H "xc-token: DEIN_TOKEN" https://noco.example.de/api/v2/meta/bases/BASE_ID/tables
```

### Zuordnung

| Feld | Bedeutung |
| --- | --- |
| Titelspalte | wird die erste Zeile des Signals. Pflicht. |
| Datumsspalte | bestimmt die Dringlichkeit. Leer ⇒ alles nur `Info`. |
| Statusspalte | zusammen mit *Erledigt-Werte* |
| Erledigt-Werte | diese Zeilen erzeugen kein Signal mehr, etwa `Abgelehnt, Zurueckgezogen` |
| Filter | NocoDB-`where`, etwa `(Status,neq,Archiviert)` |

Verknuepfte Zeilen und Auswahlfelder kommen als Objekt; Tory liest daraus
`title`, `value` oder `name`. Datumsspalten versteht es als `2026-09-12`,
`2026-09-12 08:00:00+00:00`, RFC 3339 und `12.09.2026`.

Ist eine Datumsspalte gesetzt, sortiert Tory serverseitig danach — sonst
schneidet `limit` willkuerlich ab.

Mehrere Tabellen landen in **einer** Karte, mit einer Zeile je Tabelle.

---

## 4 · Gmail — lesend

### Einmalig bei Google

1. [console.cloud.google.com](https://console.cloud.google.com) → Projekt anlegen.
2. *APIs & Dienste* → **Gmail API** aktivieren.
3. *OAuth-Zustimmungsbildschirm* → **Extern**, im Status *Test*, und dich selbst
   als Testnutzer eintragen. **Nicht** veroeffentlichen: als Testnutzer brauchst
   du keine App-Pruefung durch Google.
4. *Anmeldedaten* → OAuth-Client-Id, Typ **Desktop** (oder **Android** mit
   Paketname `de.tory.app`).

Ein Client-Secret wird nicht gebraucht und nicht eingegeben: PKCE ersetzt es.

| Feld in Tory | Wert |
| --- | --- |
| Client-Id | `…apps.googleusercontent.com` |
| Rueckleitung | `de.tory.app://oauth2` |

Dann *Bei Google anmelden* — die App oeffnet den Systembrowser, und die
Rueckleitung landet per Deep Link wieder in Tory.

### Android: der Rueckkanal braucht einen Eintrag

`tauri android init` erzeugt `app/src-tauri/gen/android/`. Damit Android
`de.tory.app://` an Tory ausliefert, gehoert in die `AndroidManifest.xml` in die
`MainActivity`:

```xml
<intent-filter>
  <action android:name="android.intent.action.VIEW" />
  <category android:name="android.intent.category.DEFAULT" />
  <category android:name="android.intent.category.BROWSABLE" />
  <data android:scheme="de.tory.app" />
</intent-filter>
```

Der Ordner `gen/` ist nicht im Repository — er wird erzeugt, und dieser Eintrag
ist nach einem `init` erneut faellig. Falls der Deep Link einmal nicht ankommt:
die Rueckleitungs-URL laesst sich von Hand einfuegen, dafuer gibt es den Befehl
`finish_gmail_auth`.

### Suchen

Jede Suche wird eine eigene Signalgruppe mit eigener Dringlichkeit:

| Beschriftung | Suche | Dringlichkeit |
| --- | --- | --- |
| Wichtig & ungelesen | `is:unread is:important newer_than:14d` | Heute |
| Direkt an mich | `is:unread category:primary -is:important newer_than:3d` | Bald |
| Rechnungen | `is:unread (Rechnung OR Zahlung) newer_than:30d` | Jetzt |

Gelesen werden nur `From`, `Subject`, `List-Id` und Googles `snippet` —
ausdruecklich kein Nachrichtentext. Mail mit `List-Id` gilt als Newsletter und
bleibt `Info`, auch wenn die Suche „Jetzt" sagt.

Umfang ist `gmail.readonly`. Tory markiert nichts als gelesen und sendet nichts.

---

## 5 · Nachrichten — RSS und Atom

Alle Feeds sind **eine** Quelle, damit `headline_limit` ueber alle zusammen
greift und der Startscreen eine Nachrichtenkarte hat statt zwanzig.

Drei Arten, die unterschiedlich behandelt werden:

| Art | Dringlichkeit | Beispiele |
| --- | --- | --- |
| Regional | `Info` | Lokalzeitung, Stadt, Verkehrsverbund |
| Welt | `Info` | tagesschau, heise, Golem |
| **Wetter** | `Hoch` | DWD-Warnlage, Unwetterzentrale |

Wetter ist absichtlich hoeher: eine Unwetterwarnung ist eine Warnung, keine
Schlagzeile.

Ein paar Feeds, die vollstaendige Inhalte liefern:

```
https://www.tagesschau.de/index~rss2.xml           Welt
https://www.heise.de/rss/heise-atom.xml            Welt
https://rss.golem.de/rss.php?feed=RSS2.0           Welt
https://www.dwd.de/DWD/warnungen/warnapp/json/     Wetter (Warnlage, je Bundesland)
```

Einstellbar sind **Schlagzeilen auf dem Startscreen** (Vorgabe 10) und
**Hoechstalter** (Vorgabe 36 h). Dieselbe Meldung aus zwei Feeds erscheint
einmal — verglichen wird der Titel ohne Satzzeichen und Grossschreibung.

Ein toter Feed nimmt die anderen nicht mit; die Karte sagt dann „3 von 5 Feeds
erreichbar". Erst wenn **kein** Feed antwortet, gilt der Sync als
fehlgeschlagen.

---

## 6 · AI — Roadmap, aber schon nutzbar

Siehe `roadmap-ki.md` fuer das Warum und das, was noch kommt.

| Anbieter | Schluessel | Basis-URL |
| --- | --- | --- |
| Anthropic | `ai.anthropic.key` | Vorgabe `https://api.anthropic.com` |
| OpenAI | `ai.openai.key` | Vorgabe `https://api.openai.com` |
| Ollama | keiner | `http://192.168.1.20:11434` |

**„Nur Titel und Zeiten senden"** ist die Vorgabe und sollte es bleiben: das
Modell sieht dann Titel, Quelle, Zeitpunkt und Dringlichkeit — genug, um zu
priorisieren, zu wenig, um Mailinhalte oder Notiztexte auszuplaudern.

Wer das abschaltet, sendet zusaetzlich die Auszuege. Bei Ollama im eigenen Netz
ist das unkritisch; bei einem Dienst im Internet ist es eine Entscheidung, die
man bewusst trifft.

---

## Wenn etwas nicht geht

| Anzeige | Bedeutung |
| --- | --- |
| stiller Graustich | kein Netz. Kein Fehler, nur alte Zahlen. |
| „Anmeldung abgelaufen" mit Knopf | Token weg oder Zugriff entzogen. Neu anmelden. |
| „Nicht gefunden (404)" | Basis-URL oder Tabellen-Id stimmt nicht. |
| „Konfiguration: …" | Ein Feld fehlt oder passt nicht. Wird **nicht** im Takt wiederholt. |
| „Kontingent erschoepft" | Zu viele Aufrufe. Tory wartet bis `retry_after`. |

Der Sync-Stand auf jeder Karte sagt, wie alt die Zahlen sind. Aelter als zwei
Stunden wird die Karte sichtbar blass — damit nie ein alter Stand als aktueller
durchgeht.

Wo die Daten liegen, steht in den Einstellungen unter *Ueber*.

---

## Anhang · Release-Signierung

Der CI-Lauf baut ein Debug-APK, weil es sofort installierbar ist. Wer ein
Release-APK will — kleiner, schneller, aber mit abgeschaltetem Cleartext —
braucht einen eigenen Keystore:

```bash
keytool -genkey -v -keystore tory.jks -keyalg RSA -keysize 2048 \
        -validity 10000 -alias tory
```

Der Keystore gehoert **nicht** ins Repository. In GitHub unter *Settings →
Secrets → Actions* hinterlegen:

| Secret | Inhalt |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | `base64 -w0 tory.jks` |
| `ANDROID_KEYSTORE_PASSWORD` | das Keystore-Passwort |
| `ANDROID_KEY_ALIAS` | `tory` |
| `ANDROID_KEY_PASSWORD` | das Schluesselpasswort |

Im Lauf danach vor dem Build eine `keystore.properties` neben
`gen/android/app/` schreiben und den `signingConfigs`-Block in
`app/build.gradle.kts` ergaenzen — beides beschreibt die Tauri-Dokumentation
unter *Distribute → Google Play*.

Zwei Dinge, die dabei auffallen werden:

* **Cleartext.** Ein Release-Build setzt `usesCleartextTraffic` auf `false`.
  Laufen Mindwtr oder NocoDB im eigenen Netz ueber `http://`, antwortet Tory
  danach mit „kein Netz". Entweder die Dienste hinter HTTPS legen (Caddy macht
  das mit zwei Zeilen) oder eine `network_security_config.xml` mit den eigenen
  Hosts hinterlegen.
* **Der Deep Link bleibt noetig.** `scripts/android-manifest.mjs` laeuft auch im
  Release-Pfad, sonst kommt die Gmail-Anmeldung nicht zurueck.

Fuer ein Telefon lohnt der Aufwand selten. Das Debug-APK tut dasselbe.
