# Tory

Persönliches Dashboard als Android-App (Samsung Galaxy, One UI). Ein Screen für
Mail, Pakete, Konten, Aufgaben, Termine und Nachrichten — statt acht App-Starts
am Morgen.

**Stand: Konzeptphase.** Es gibt noch keinen Code, sondern einen ausgearbeiteten
Entwurf und drei Designrichtungen zur Auswahl.

| Was | Wo |
| --- | --- |
| Produktkonzept, Module, Integrationen, Roadmap | [`docs/konzept.md`](docs/konzept.md) |
| Technische Architektur, Modulschnitt, Sync, Secrets | [`docs/architektur.md`](docs/architektur.md) |
| Die drei Designrichtungen im Vergleich | [`docs/design-richtungen.md`](docs/design-richtungen.md) |
| Schnittstellendefinition (Datenmodell, Modulvertrag) | [`docs/schnittstellen.md`](docs/schnittstellen.md) |
| Kotlin-Verträge + Abdeckungsprüfung | [`contracts/`](contracts/) |
| Interaktive Mockups (Artifact-Seite) | [`mockups/tory-mockups.html`](mockups/tory-mockups.html) |

## Nächster Schritt

Eine der drei Richtungen (A · Ruhiger Morgen, B · Cockpit, C · Bento) auswählen.
Danach entsteht M0: Gradle-Projekt mit Modulschnitt, Compose-Theme in der
gewählten Richtung und Startscreen mit Beispieldaten.

## Hinweis zu den Mockups

`mockups/tory-mockups.html` ist ein HTML-Fragment ohne `<html>`/`<head>`/`<body>`
— es wird als Claude-Artifact gerendert. Zum lokalen Ansehen in ein Grundgerüst
mit `<!doctype html><html><head>…</head><body>` einbetten.
