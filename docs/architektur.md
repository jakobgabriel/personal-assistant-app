# Tory — Technische Architektur

## Stack

| Bereich | Wahl | Warum |
| --- | --- | --- |
| Sprache/UI | Kotlin, Jetpack Compose, Material 3 | Standard auf Android, gute One-UI-Nähe, schnelle Iteration |
| Persistenz | Room | Einzige Wahrheit auf dem Gerät, Flows in die UI |
| Hintergrund | WorkManager | Überlebt One-UIs aggressives Prozess-Beenden, anders als Foreground-Services |
| Netzwerk | Ktor Client + kotlinx.serialization | Leichtgewichtig, gute Coroutine-Integration |
| DI | Hilt | Kein Boilerplate, gute Compose-Integration |
| Widgets | Glance | Compose-Syntax für Homescreen-Widgets, teilt Daten mit der App |
| Auth | AppAuth (OAuth 2 / PKCE) | Gmail, Google Tasks, PSD2 |
| Secrets | EncryptedSharedPreferences + Android Keystore | Tokens hardwaregestützt verschlüsselt |

Minimum SDK 30, Ziel 34+ (One UI 6).

## Modulschnitt

```
:app                  Navigation, Startscreen, Zusammenbau, Widgets
:core:design          Design-Tokens, Komponenten, gewählte Designrichtung
:core:data            Ereignismodell, Room, Repositories, Sync-Registry
:core:network         Ktor, OAuth, Retry-/Fehlerlogik
:feature:mail
:feature:pakete
:feature:geld
:feature:aufgaben
:feature:news
:feature:kalender
```

Jedes `:feature:*` erfüllt denselben Vertrag:

```kotlin
interface SignalQuelle {
    val id: QuellenId
    fun signale(): Flow<List<Signal>>       // fuer den Startscreen
    suspend fun sync(): SyncErgebnis        // vom WorkManager gerufen
}
```

Der Startscreen kennt nur `Signal` — keine Domänenlogik. Ein neues Modul
anzubinden heißt: `SignalQuelle` implementieren und registrieren.

## Ereignismodell

```kotlin
data class Signal(
    val id: String,
    val quelle: QuellenId,          // MAIL, PAKET, GELD, KALENDER, AUFGABE, NEWS
    val titel: String,
    val untertitel: String?,
    val zeitpunkt: Instant?,        // Termin, Zustellfenster, Abbuchungsdatum
    val dringlichkeit: Dringlichkeit,  // KRITISCH, HOCH, NORMAL, INFO
    val betrag: Money?,
    val aktion: Aktion?,            // Deep Link in die Ziel-App
)
```

Sortierung im Startscreen: `dringlichkeit`, dann Nähe von `zeitpunkt` zu jetzt.
Nichts anderes.

## Synchronisation

Ein `PeriodicWorkRequest` pro Quelle, unterschiedliche Takte:

| Quelle | Takt | Bedingung |
| --- | --- | --- |
| Mail | 15 min | Netz vorhanden |
| News | 30 min | Netz vorhanden, nicht im Sparmodus |
| Pakete | 1 h | Netz vorhanden |
| Konten | 6 h | Netz + Akku > 20 % |
| Kalender | Content-Observer | ereignisgesteuert, kein Polling |

Jede Quelle speichert `letzterSync` und `letzterFehler` — der Startscreen zeigt
den Stand an, statt veraltete Zahlen als aktuell auszugeben. Fehlgeschlagene
Syncs laufen mit exponentiellem Backoff nach.

Zusätzlich ein einmaliger `ExpeditedWork` beim Öffnen der App, wenn der letzte
Sync älter als fünf Minuten ist.

## Secrets und die „Tory Bridge"

Bank-Aggregatoren (GoCardless Bank Account Data, finAPI, Enable Banking) geben
`secret_id` / `secret_key` aus. Diese Werte dürfen **nicht** in die APK — eine
APK lässt sich entpacken.

Deshalb ein minimaler eigener Dienst:

```
Telefon  ──(kurzlebiges Token)──>  Tory Bridge (Ktor, eigener Server)  ──>  Aggregator
```

* Die Bridge hält die Aggregator-Zugangsdaten und den Refresh-Flow.
* Sie speichert **keine** Umsätze — sie reicht sie durch, das Telefon persistiert.
* Absicherung: ein einziger Geräteschlüssel, mTLS oder ein Bearer-Token aus dem Keystore.
* Alles andere (Gmail, Kalender, RSS, Wetter, Paket-APIs) läuft direkt vom
  Telefon — keine Bridge nötig.

Alternative ohne eigenen Server: FinTS/HBCI direkt, dann liegt die Banking-PIN
im Keystore und es gibt keinen Dritten. Kostet eine Produktregistrierung.

## Samsung-Besonderheiten

* **Modi und Routinen** — „Morgens 7:30" kann per Intent den Sync anstoßen und den Morgenbrief auslösen.
* **Always-On-Display** — nächstes Signal als eine Zeile.
* **Edge-Panel** — Aufgabe schnell erfassen, ohne die App zu öffnen.
* **Akku-Optimierung** — One UI beendet Hintergrunddienste härter als Stock-Android. Deshalb WorkManager statt eigener Services, und die App einmalig von der Akku-Optimierung ausnehmen lassen.
* **Health Connect** — Samsung Health schreibt dorthin; der direkte Samsung-Health-SDK-Zugang ist für Einzelentwickler gesperrt.

## Test

* Repositories gegen In-Memory-Room, Quellen gegen aufgezeichnete Antworten.
* Ein `FakeSignalQuelle`-Satz liefert den Beispieltag aus den Mockups — damit
  lässt sich der Startscreen ohne jede Anbindung entwickeln und in Screenshot-
  Tests festhalten.
