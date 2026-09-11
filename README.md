# Tory

Persoenliches Dashboard als Tauri-App fuer Android (und Desktop). Ein Screen
fuer Notizen, Aufgaben, Tabellen, Mail und Nachrichten — statt acht App-Starts
am Morgen.

Angebunden sind, alle frei konfigurierbar und in mehreren Instanzen:

| Quelle | Woher | Was daraus wird |
| --- | --- | --- |
| **Obsidian** — beliebig viele Vaults | Ordner oder WebDAV | offene Checkboxen mit Frist, angepinnte Notizen |
| **Mindwtr** — selbst gehostet | REST unter `/v1` | Aufgaben nach Status, Faelligkeit, Tagesfokus |
| **NocoDB** — selbst gehostet | API v2, `xc-token` | beliebige Tabellen ueber eine Spaltenzuordnung |
| **Gmail** | Gmail API, lesend | Ungelesenes je Suchausdruck |
| **Nachrichten** | RSS / Atom / JSON Feed | regional, weltweit, Wetter |

Dazu eine AI-Schicht (Anthropic, OpenAI, Ollama) mit eigenen Schluesseln: der
erste Schritt, das Tagesbriefing, laeuft.

## Stand

M0 bis M2 sind gebaut: Kern, alle fuenf Anbindungen, Oberflaeche mit
vollstaendigen Einstellungen. Der Kern hat 106 Tests gegen aufgezeichnete
Antworten der Dienste, die Rust-Seite ist clippy-sauber, die Oberflaeche
typprueft ohne Fehler.

Was fehlt, ist die Android-Uebersetzung selbst (`tauri android init`,
Signierung, Geraetetest) — dafuer braucht es ein Android-SDK, siehe unten.

## Aufbau

```
crates/tory-core/    Rust-Kern: Modell, Anbindungen, SQLite, Sync, AI
                     — ohne Tauri, deshalb ohne Emulator testbar
app/                 Tauri-App
  src/               Svelte 5 + TypeScript
  src-tauri/         Schale: Fenster, Zustand, Befehle
docs/                Konzept, Architektur, Einrichtung, Roadmap
mockups/             Die drei Designrichtungen als Artifact-Seite
archiv/              Der abgeloeste Kotlin-Entwurf
```

Die Trennung ist der Kern der Sache: in `app/src-tauri` steht keine Entscheidung,
jeder Befehl ist eine Zeile Weiterleitung. Was entschieden wird, liegt in
`tory-core` — und laesst sich ohne Android-SDK, ohne WebView und ohne Netz
pruefen.

| Was | Wo |
| --- | --- |
| Produktkonzept, Quellen, Reihenfolge | [`docs/konzept.md`](docs/konzept.md) |
| Technische Architektur | [`docs/architektur.md`](docs/architektur.md) |
| **Quellen einrichten — Schritt fuer Schritt** | [`docs/integrationen.md`](docs/integrationen.md) |
| Datenmodell und Vertraege | [`docs/schnittstellen.md`](docs/schnittstellen.md) |
| AI und was noch kommt | [`docs/roadmap-ki.md`](docs/roadmap-ki.md) |
| Die drei Designrichtungen | [`docs/design-richtungen.md`](docs/design-richtungen.md) |

## Bauen

### Voraussetzungen

Rust (stabil), Node 20+, und fuer den Desktop die ueblichen Tauri-Pakete —
unter Ubuntu/Debian:

```bash
sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev \
                 build-essential curl wget file libssl-dev
```

### Kern pruefen (braucht nichts davon)

```bash
cargo test -p tory-core        # 106 Tests, ohne Netz
cargo clippy --all-targets
```

### Desktop

```bash
cd app
npm install
npm run tauri dev              # oder: npm run tauri build
```

Der Desktop ist die schnelle Schleife: dieselbe Oberflaeche, derselbe Kern,
ohne Emulator. Das Fenster ist bewusst telefonschmal.

### Android

```bash
# einmalig: Android SDK + NDK, dann
export ANDROID_HOME=$HOME/Android/Sdk
export NDK_HOME=$ANDROID_HOME/ndk/<version>
rustup target add aarch64-linux-android armv7-linux-androideabi \
                  i686-linux-android x86_64-linux-android

cd app
npm run tauri android init
npm run tauri android dev      # oder: android build --apk
```

Nach `android init` gehoert der Intent-Filter fuer `de.tory.app://` in die
erzeugte `AndroidManifest.xml` — sonst kommt die Gmail-Anmeldung nicht
zurueck. Der genaue Ausschnitt steht in
[`docs/integrationen.md`](docs/integrationen.md#4--gmail--lesend).

## Wo die Daten liegen

Alles im privaten Datenverzeichnis der App (in den Einstellungen unter *Ueber*
nachzulesen):

```
config.json            URLs, Vault-Namen, Feeds, Takte — lesbar, weitergebbar
secrets/secrets.bin    Tokens und API-Schluessel, AES-256-GCM
secrets/device.key     der Geraeteschluessel
tory.sqlite3           Signale, Uebersichten, Sync-Stand
```

`config.json` enthaelt nie ein Geheimnis, sondern nur dessen Namen. Deshalb
laesst sie sich sichern und weitergeben, ohne etwas herauszuschneiden.

Zur Einordnung: der Geraeteschluessel liegt neben den Daten. Das schuetzt gegen
andere Apps, nicht gegen jemanden mit Dateizugriff auf ein entsperrtes Geraet.
Der Android Keystore steht in [`docs/roadmap-ki.md`](docs/roadmap-ki.md) unter
den offenen Punkten.

## Hinweis zu den Mockups

`mockups/tory-mockups.html` ist ein HTML-Fragment ohne `<html>`/`<head>`/`<body>`
— es wird als Claude-Artifact gerendert. Zum lokalen Ansehen in ein Grundgeruest
einbetten. Die Mockups zeigen noch den frueheren Quellensatz (Pakete, Konten);
als Vergleich der drei Designrichtungen gelten sie weiter. Gebaut ist Richtung B
(„Cockpit"), Richtung A („Ruhiger Morgen") liegt als helles Thema daneben und
ist in den Einstellungen umschaltbar.
