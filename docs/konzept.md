# Tory — Produktkonzept

## Problem

Der Morgen-Rundlauf: Gmail öffnen, Kalender prüfen, DHL-App, Banking-App,
Nachrichten-App, Notizen. Sechs bis acht Starts für eine Frage, die eine
Antwort hat: *Was ist heute wichtig?*

Bestehende Aggregatoren scheitern daran, dass sie alles gleich laut anzeigen —
sie ersetzen acht Feeds durch einen längeren.

## Leitidee: Signal vor Detail

Jede angebundene Quelle liefert nicht ihren Inhalt, sondern ihre **Signale**:
Dinge mit Termin, Betrag oder Handlungsbedarf.

* „DHL kommt heute 14–16 Uhr" ist ein Signal.
* „Sie haben 214 E-Mails" ist keins.

Der Startscreen zeigt maximal drei Signale, danach den Tag, danach die
Domänenkarten. Alles Übrige liegt eine Ebene tiefer.

## Drei Prinzipien

1. **Ein Ereignismodell.** Mail, Sendung, Buchung, Termin und Aufgabe werden
   intern derselbe Typ: `id, titel, untertitel, zeitpunkt, dringlichkeit,
   quelle, aktion`. Der Startscreen sortiert nur noch — er kennt keine
   Sonderfälle pro Domäne.
2. **Offline zuerst.** Room ist die einzige Wahrheit auf dem Gerät. Die
   Oberfläche liest nie direkt vom Netz; die App startet mit dem letzten Stand
   und synchronisiert im Hintergrund.
3. **Lesend, nicht handelnd.** Tory führt keine Überweisungen aus und schreibt
   keine Mails. Aktionen delegiert es an die Ziel-App (Gmail, Banking). Das
   spart Lizenzfragen, Verantwortung und sehr viel Aufwand.

## Module

| Modul | Stufe | Inhalt | Quellen |
| --- | --- | --- | --- |
| Heute | Start | Signalzeile, Termine, fällige Aufgaben, Domänenkarten | alle |
| Mail | MVP | Fokus-Postfach: ungelesen & wichtig, Newsletter/Rechnungen gebündelt | Gmail API, IMAP |
| Aufgaben | MVP | Titel, optionales Datum, Liste. Aus Mail/Paket erzeugbar | lokal (Room) |
| Kalender | MVP | Heute und morgen, mit Reisezeit zum nächsten Termin | CalendarContract |
| News | MVP | Eigene Quellen als RSS, entdoppelt, auf 10 Schlagzeilen begrenzt | RSS/Atom |
| Pakete | v1 | Sendungen aus Versandmails erkannt, Status per API, Zustellfenster als Signal | Mail-Parser, DHL/17TRACK |
| Geld | v1 | Kontostände, kommende Abbuchungen, Monatsbudget, Umsätze nach Kategorie | PSD2-Aggregator oder FinTS |
| Puls | v2 | Schritte, Schlaf, Trainings als eine Zeile Kontext | Health Connect |
| Wetter & Weg | v2 | Regenwarnung, Störungen auf üblichen Strecken | Bright Sky (DWD), DB/HAFAS |

## Aufbau des Startscreens

1. **Gruß & Datum** — plus Sync-Stand, damit klar ist, wie alt die Zahlen sind.
2. **Jetzt wichtig** — max. drei Signale, quellenübergreifend nach Dringlichkeit.
3. **Heute** — Termine und fällige Aufgaben.
4. **Domänen** — Mail, Pakete, Geld, News als Karten in frei wählbarer Reihenfolge.

### Navigation

* **Reiter:** Heute · News · Mail · Geld · Mehr (Pakete, Aufgaben, Puls, Einstellungen)
* **Widget:** dieselbe Signalzeile als Glance-Widget, 4×1 und 4×2
* **Gedrückt halten:** je Karte erledigt / später / in Aufgabe verwandeln / Quelle öffnen
* **Morgenbrief:** eine Benachrichtigung um 7:30 mit genau den Signalen des Tages

## Integrationen und ihre Haken

| Bereich | Quelle | Machbarkeit | Haken |
| --- | --- | --- | --- |
| Mail | Gmail API (read-only) | einfach | Eigenes Google-Cloud-Projekt, du als Testnutzer — keine App-Prüfung nötig |
| Mail (weitere) | IMAP | einfach | IDLE kostet Akku; besser alle 15 Minuten abfragen |
| Kalender | CalendarContract | einfach | Nutzt die Konten, die auf dem Gerät schon eingerichtet sind |
| News | RSS/Atom | einfach | tagesschau, heise, Golem liefern vollständige Feeds |
| Wetter | Bright Sky (DWD) | einfach | Offen, kein Schlüssel, sehr gute Regenprognose für DE |
| Aufgaben | Room, optional Google Tasks | einfach | Erst lokal, Sync später |
| Pakete · Erkennung | Versandmails | mittel | Pro Händler ein Erkennungsmuster, muss gepflegt werden |
| Pakete · Status | DHL API, 17TRACK/AfterShip | mittel | DHL allein kostenlos; Aggregatoren mit knappen Freikontingenten |
| Konten | PSD2 (GoCardless Bank Account Data) | mittel | Kostenlos, aber 90-Tage-Reconsent; Secrets gehören nicht in die App → Bridge |
| Konten (Alt.) | FinTS/HBCI | mittel | Direkt zur Bank, braucht Produktregistrierung |
| Puls | Health Connect | mittel | Samsung Health schreibt dorthin; direkter SDK-Zugang ist gesperrt |
| Weg | DB/HAFAS | mittel | Nur für zwei, drei feste Strecken sinnvoll |
| Überweisungen | — | nicht vorgesehen | Zahlungsauslösung braucht BaFin-Lizenz |

## Datenschutz

* Kein eigenes Cloud-Backend für Inhalte. Mails, Termine, Umsätze bleiben auf dem Gerät.
* Keine Analytics, kein Crash-Reporting mit Inhalten.
* Der Geld-Reiter liegt hinter Biometrie.
* Datenexport als JSON, Löschen pro Quelle.
* Einzige Ausnahme: die *Tory Bridge* für Bank-Zugänge (siehe `architektur.md`) —
  sie hält Tokens, aber keine Umsätze.

## Reihenfolge

| Stufe | Dauer | Ergebnis |
| --- | --- | --- |
| M0 · Gerüst | 1 Woche | Projekt, Modulschnitt, Theme, Startscreen mit Beispieldaten |
| M1 · Erste echte Daten | 2 Wochen | Kalender, Aufgaben, News, Gmail-Fokus — ersetzt vier App-Starts |
| M2 · Pakete & Widget | 1–2 Wochen | Versandmails erkennen, Status abrufen, erstes Homescreen-Widget |
| M3 · Geld | 2 Wochen | Bridge, Konten, Kategorisierung, Budget, Biometrie |
| M4 · Feinschliff | offen | Puls, Wetter, Wege, Morgenbrief als Routine, Export |

Jede Stufe endet mit etwas Benutzbarem — nicht mit einem halben Feature.
