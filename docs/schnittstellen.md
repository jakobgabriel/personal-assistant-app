# Tory — Schnittstellendefinition

Der Vertrag zwischen Datenquellen, Modulen und Dashboard. Ziel: **jede
Information, die in den Mockups sichtbar ist, hat genau ein Feld im Modell** —
und keine Domäne braucht Sonderbehandlung im Startscreen.

Quellen: [`contracts/src/main/kotlin/de/tory/core/`](../contracts/src/main/kotlin/de/tory/core/)
Prüfung: `python3 contracts/pruefe_abdeckung.py`

## Die drei Ebenen

```
Quelle (Gmail, DHL, Bank, RSS)
   |  DomaeneModul.sync()          -> schreibt in Room
   v
Element (MailNachricht, Sendung, Konto, Umsatz, Aufgabe, Termin, ...)
   |  DomaeneModul.signale()       -> was heute Aufmerksamkeit braucht
   |  DomaeneModul.uebersicht()    -> Karte / Kennzahl / Kachel
   v
Startseite (Signal, DomaenenUebersicht)  -> die Oberflaeche liest nur das
```

Die Oberfläche kennt **Element-Typen nur auf den Detailseiten**. Der
Startscreen kennt ausschließlich `Signal` und `DomaenenUebersicht` — deshalb
ändert ein neues Modul den Startscreen nicht.

## 1 · Signal — der gemeinsame Nenner

Alles, was auf den Startscreen darf, ist ein `Signal`: etwas mit Zeitpunkt,
Betrag oder Handlungsbedarf.

| Feld | Wofür in den Mockups |
| --- | --- |
| `titel`, `untertitel` | die zwei Zeilen jeder Signalzeile |
| `dringlichkeit` | Farbe des Statusbalkens (B), Sortierung überall |
| `zeitpunkt` + `zeitpunktArt` | „in 5 h" vs. „FRIST 1 T" vs. „fällig am" |
| `zeitfenster` | „14–16 Uhr" — Zustellfenster brauchen eine Spanne, keinen Punkt |
| `betrag` | „EUR 1.180,00" |
| `kennung` | Sendungsnummer, IBAN-Endung, Ort |
| `aktion` | was beim Antippen passiert |
| `erledigbar`, `stummBis` | abhaken bzw. „später" direkt in der Zeile |

`zeitpunktArt` ist der Grund, warum die App aus einem `Instant` die richtige
Formulierung ableiten kann, ohne pro Domäne eine Sonderregel zu haben.

## 2 · DomaenenUebersicht — eine Struktur für drei Designrichtungen

Dieselben Daten bedienen die Karte in A, die Kennzahlenzeile in B und die
Kachel in C. Die Richtung entscheidet nur, welche Felder sie zeigt:

| Feld | A · Karte | B · Kennzahlenleiste | C · Kachel |
| --- | --- | --- | --- |
| `kennzahl` / `bezeichnung` | Kartentitel-Zusatz | die Zahl in der Leiste | große Zahl |
| `zusatz` | — | — | „1 mit Frist" |
| `zeilen` (`Kurzzeile`) | Zeilen der Karte | — | Kachelinhalt |
| `trend` | — | Sparkline | — |
| `fortschritt` | — | Budgetbalken | Balken / Punktkette |
| `stand` | „vor 2 Min" | „SYNC VOR 2 MIN" | Grauton bei Veralterung |

`kennzahl` ist bereits formatiert (`"3.428,60 €"`), `kennzahlRoh` derselbe Wert
als Zahl für Schwellen und Sortierung. Formatierung passiert einmal im Modul,
nicht dreimal in drei Designrichtungen.

## 3 · DomaeneModul — der Vertrag pro Domäne

```kotlin
interface DomaeneModul {
    val id: QuellenId
    fun signale(): Flow<List<Signal>>
    fun uebersicht(): Flow<DomaenenUebersicht>
    fun elemente(filter: Filter = Filter.KEINER): Flow<List<DomaenenElement>>
    fun status(): StateFlow<QuellenStatus>
    suspend fun sync(anlass: SyncAnlass): SyncErgebnis
    suspend fun anmelden(): AnmeldeErgebnis
    suspend fun abmelden()
}
```

Regeln, die den Vertrag tragen:

* `sync()` wird **nur** vom WorkManager gerufen, nie aus der Oberfläche.
* `signale()` und `uebersicht()` lesen ausschließlich aus Room — offline sofort da.
* `abmelden()` löscht Tokens *und* alle lokalen Daten der Quelle.
* Ein Modul, das nichts zu melden hat, liefert eine leere Signalliste — nicht null.

## 4 · Aktualität ist Teil der Daten

`SyncStand` hängt an jeder Übersicht und an der Startseite. Damit kann die App
nie alte Zahlen als aktuell ausgeben. `SyncFehler` unterscheidet die Fälle, die
unterschiedlich aussehen müssen:

| Fehler | Anzeige |
| --- | --- |
| `KeinNetz` | stiller Graustich, kein Banner |
| `AnmeldungAbgelaufen` | Banner mit Knopf — der PSD2-90-Tage-Fall |
| `KontingentErschoepft` | Hinweis mit `wiederAb`, kein Wiederholungsversuch |
| `Serverfehler`, `Unbekannt` | Hinweis in den Einstellungen, Backoff |

## 5 · Abdeckungsprüfung

[`contracts/abdeckung.json`](../contracts/abdeckung.json) hält für jedes
sichtbare Element der Mockups fest, aus welchen Feldern es kommt.
`pruefe_abdeckung.py` liest die Kotlin-Quellen und prüft jeden Verweis:

```
$ python3 contracts/pruefe_abdeckung.py
5 Kotlin-Dateien, 68 Typen
71 Mockup-Elemente, 161 Feldverweise geprueft

OK - jede Information aus den Mockups hat ein Feld im Modell.
```

Der Sinn: Wird ein Mockup erweitert oder ein Feld umbenannt, fällt die Lücke
sofort auf, statt erst beim Bauen des Screens. Gehört in die CI, sobald es eine gibt.

## Bewusst nicht im Modell

* **Kein Volltext von Mails.** Nur Betreff und Auszug — für alles Weitere
  öffnet Tory Gmail. Spart Speicher, Synchronisation und Risiko.
* **Keine vollständige IBAN.** Nur `Konto.ibanEndung`, vier Stellen.
* **Keine Schreiboperationen nach außen.** Kein Senden, kein Überweisen. Die
  einzigen Schreibvorgänge sind lokal: Aufgaben, Reihenfolge, Stummschaltung.
* **Kein Nutzerkonto, keine Geräte-Synchronisation.** Ein Telefon, eine Datenbank.

## Offene Entscheidungen

1. **`Geld` als Long-Cent** ist gesetzt. Offen ist, ob Fremdwährungen
   überhaupt vorkommen — falls nicht, kann `waehrung` später entfallen.
2. **Kategorien** (`Kategorie.schluessel`) brauchen einen festen Satz, sobald
   das Geld-Modul kommt. Vorschlag: die zehn Kategorien der Budgetansicht,
   Rest auf „Sonstiges".
3. **`Termin.reisezeit`** braucht eine Routing-Quelle. Bis M4 bleibt das Feld
   null, die UI zeigt den Ort dann ohne Fahrtzeit.
