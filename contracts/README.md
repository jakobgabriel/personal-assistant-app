# contracts

Die Schnittstellendefinition der App als Kotlin-Quellen — noch ohne
Gradle-Projekt, damit sie unabhängig vom späteren Modulschnitt gelesen und
geprüft werden kann. Beim Anlegen von M0 wandert `src/main/kotlin/` nach
`:core:data` bzw. `:core:api`.

```
src/main/kotlin/de/tory/core/
  model/Signal.kt       Signal, QuellenId, Dringlichkeit, Geld, Aktion
  model/Uebersicht.kt   DomaenenUebersicht, Trend, Fortschritt, Startseite
  model/Elemente.kt     MailNachricht, Sendung, Konto, Umsatz, Aufgabe, Termin, ...
  model/Sync.kt         SyncStand, SyncFehler, SyncErgebnis, QuellenStatus
  api/Schnittstellen.kt DomaeneModul, DashboardRepository, WidgetRepository, Einstellungen
```

## Abdeckungsprüfung

`abdeckung.json` ordnet jedem sichtbaren Element der Mockups seine Felder zu.
Das Skript prüft, dass jedes genannte Feld in den Kotlin-Quellen existiert:

```
python3 contracts/pruefe_abdeckung.py
```

Exitcode 0 heißt: keine Information aus den Mockups ohne Feld im Modell.
Beim Erweitern der Mockups zuerst hier den Eintrag ergänzen — das Skript zeigt
dann, welches Feld noch fehlt.

Erläuterung des Modells: [`../docs/schnittstellen.md`](../docs/schnittstellen.md)
