# Tory — Produktkonzept

## Problem

Der Morgen-Rundlauf: Gmail oeffnen, Mindwtr pruefen, zwei Obsidian-Vaults
durchsehen, NocoDB aufrufen, drei Nachrichtenseiten. Sechs bis acht Starts fuer
eine Frage, die eine Antwort hat: *Was ist heute wichtig?*

Bestehende Aggregatoren scheitern daran, dass sie alles gleich laut anzeigen —
sie ersetzen acht Feeds durch einen laengeren.

## Leitidee: Signal vor Detail

Jede angebundene Quelle liefert nicht ihren Inhalt, sondern ihre **Signale**:
Dinge mit Termin, Frist oder Handlungsbedarf.

* „Bewerbungsfrist laeuft morgen ab" ist ein Signal.
* „Du hast 214 E-Mails" ist keins.

Der Startscreen zeigt hoechstens drei Signale, danach die Quellenkarten, danach
den Rest. Alles Weitere liegt eine Ebene tiefer — oder in der Quelle selbst.

## Drei Prinzipien

1. **Ein Ereignismodell.** Eine Obsidian-Checkbox, eine Mindwtr-Aufgabe, eine
   NocoDB-Zeile, eine Mail und eine Schlagzeile werden intern derselbe Typ:
   `Signal`. Der Startscreen sortiert nur noch — er kennt keine Sonderfaelle pro
   Quelle.
2. **Offline zuerst.** SQLite ist die einzige Wahrheit auf dem Geraet. Die
   Oberflaeche liest nie direkt vom Netz; die App startet mit dem letzten Stand
   und synchronisiert im Hintergrund.
3. **Lesend, mit genau einer Ausnahme.** Tory schreibt keine Mails und aendert
   keine Notizen. Der einzige schreibende Aufruf nach aussen ist: eine
   Mindwtr-Aufgabe abhaken. Alles andere delegiert es an die Zielanwendung.

## Quellen

| Quelle | Was daraus wird | Wie |
| --- | --- | --- |
| **Obsidian**, mehrere Vaults | Offene Checkboxen mit Frist, angepinnte Notizen | Ordner oder WebDAV |
| **Mindwtr**, selbst gehostet | Aufgaben nach Status, Faelligkeit, Tagesfokus | REST unter `/v1` |
| **NocoDB**, selbst gehostet | Beliebige Tabellen ueber eine Spaltenzuordnung | API v2, `xc-token` |
| **Gmail** | Ungelesenes und Wichtiges je Suchausdruck | Gmail API, lesend |
| **Nachrichten** | Schlagzeilen: regional, weltweit, Wetter | RSS/Atom/JSON Feed |

Mehrere Instanzen sind der Normalfall, nicht die Ausnahme: zwei Vaults, drei
NocoDB-Tabellen, fuenf Gmail-Suchen. Jede bekommt eine Kennung und erscheint als
eigene Karte — oder, bei Feeds und Tabellen, als Zeile in einer gemeinsamen.

Einrichtung im Einzelnen: [`integrationen.md`](integrationen.md).

## Aufbau des Startscreens

1. **Gruss und Datum** — plus Banner, falls eine Quelle Aufmerksamkeit braucht.
2. **Tagesbriefing** — zwei Saetze vom Modell, falls AI an ist.
3. **Jetzt wichtig** — hoechstens drei Signale, quellenuebergreifend sortiert.
4. **Quellen** — eine Karte je Quelle, mit Kennzahl und Sync-Stand.
5. **Weiter** — alles Uebrige in derselben Sortierung.

Jede Signalzeile laesst sich antippen (springt in die Quelle), spaeter stellen
(3 h oder bis morgen) oder abhaken. „Spaeter" und „abgehakt" ueberleben den
naechsten Sync.

## Dringlichkeit: eine Regel fuer alle

| Faelligkeit | Stufe |
| --- | --- |
| ueberfaellig oder heute | **Jetzt** (`Critical`) |
| morgen | **Heute** (`High`) |
| in 2–3 Tagen | **Bald** (`Normal`) |
| spaeter, oder ohne Datum | **Info** |

Ausnahmen, die begruendet sind: eine Wetterwarnung ist `Hoch`, egal wann sie
kam. Ein Newsletter bleibt `Info`, auch wenn die Gmail-Suche „Jetzt" sagt. Ein
Mindwtr-Tagesfokus kommt auch ohne Datum durch.

## Datenschutz

* Kein eigenes Backend. Alle Dienste sind bereits selbst gehostet.
* Kein Analytics, kein Crash-Reporting.
* Von Mails nur Betreff, Absender und Googles `snippet` — nie der Text.
* Tokens und API-Schluessel liegen verschluesselt, getrennt von der
  Konfiguration. Die Konfiguration ist damit weitergebbar.
* Die AI-Schicht sendet standardmaessig nur Titel und Zeiten. Wer Inhalte
  senden will, schaltet das ausdruecklich ein.

## Reihenfolge

| Stufe | Ergebnis | Stand |
| --- | --- | --- |
| M0 · Kern | Modell, SQLite, Secret-Store, Takt, Connector-Vertrag | **fertig** |
| M1 · Quellen | Obsidian, Mindwtr, NocoDB, Gmail, Feeds — je mit Tests gegen echte Antworten | **fertig** |
| M2 · Oberflaeche | Startscreen, Detailseiten, vollstaendige Einstellungen, zwei Designrichtungen | **fertig** |
| M3 · Android | `tauri android init`, Signierung, Deep Link im Manifest, Geraetetest | offen |
| M4 · Morgenbrief | Benachrichtigung um 07:30 statt Knopf | offen |
| M5 · Widget | Signalzeile auf dem Homescreen, liest dieselbe Datenbank | offen |

AI laeuft quer dazu: das Tagesbriefing ist der erste Schritt und da; was folgt,
steht in [`roadmap-ki.md`](roadmap-ki.md).

Jede Stufe endet mit etwas Benutzbarem — nicht mit einem halben Feature.
