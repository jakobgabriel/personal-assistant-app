# Tory — AI und was danach kommt

## Warum AI hier ueberhaupt etwas beitraegt

Nicht, weil AI in einer App sein muss. Sondern weil Tory einen Zustand
herstellt, den ein Modell sonst erst muehsam zusammensuchen muesste: alle
Signale des Tages, quellenuebergreifend entdoppelt, nach Dringlichkeit und Zeit
sortiert, mit Zeitpunkt und Herkunft.

Das ist bereits ein guter Prompt. Was fehlt, ist jemand, der daraus zwei Saetze
macht.

Der Umkehrschluss ist genauso wichtig: solange die Signale **nicht** sauber
sind, bringt ein Modell nichts. Deshalb steht AI hinter den Anbindungen, nicht
davor.

## Was gebaut ist

**Das Tagesbriefing.** Ein Knopf auf dem Startscreen, sichtbar nur wenn AI in
den Einstellungen an ist. Er schickt die Signale an das eingestellte Modell und
zeigt die Antwort — hoechstens vier Saetze, ohne Anrede, ohne Aufzaehlung: was
heute zaehlt, was warten kann.

Dazu die gesamte Verdrahtung, die jede weitere AI-Funktion ebenfalls braucht:

| Teil | Wo |
| --- | --- |
| Anbietervertrag | `crates/tory-core/src/ai/mod.rs` |
| Anthropic, OpenAI, Ollama | `crates/tory-core/src/ai/providers.rs` |
| Prompt aus Signalen | `ai::brief_prompt` |
| Schluessel im Secret-Store | `ai.anthropic.key`, `ai.openai.key` |
| Einstellungen | Abschnitt *AI* |

Drei Anbieter, eine Schnittstelle. Ollama ist dabei, weil „laeuft im eigenen
Netz und verlaesst das Haus nicht" fuer diese Art von Daten das beste Argument
ist, das ein Anbieter haben kann.

### Drei Entscheidungen, die bewusst so sind

1. **Der Schluessel liegt im Secret-Store**, nicht in `config.json`. Die
   Konfiguration soll weitergegeben werden koennen.
2. **`titles_only` ist Vorgabe an.** Das Modell sieht Titel, Quelle, Zeit und
   Dringlichkeit — nicht die Auszuege. Genug, um zu priorisieren; zu wenig, um
   Mailinhalte auszuplaudern. Wer mehr Qualitaet will, schaltet es aus und
   weiss dann, was er teilt.
3. **Der Verbrauch kommt zurueck.** `Completion` traegt `input_tokens` und
   `output_tokens`. Sonst merkt niemand, was ein Briefing kostet.

Der Prompt ist auf 40 Signale gedeckelt und sagt dem Modell ausdruecklich, dass
es nichts erfinden darf: was nicht in der Liste steht, existiert nicht.

## Was als Naechstes kommt

### K1 · Briefing als Benachrichtigung

Das Briefing um 07:30 als Mitteilung statt als Knopf. `morning_brief_at` steht
schon in der Konfiguration; es fehlt der Wecker und
`tauri-plugin-notification`. Auf Android heisst das ein `WorkManager`-Auftrag,
kein langlebiger Dienst.

### K2 · Fragen an den Vault

„Was hatte ich zum Dachdecker notiert?" Die Notizen liegen bereits gelesen in
Tory. Was fehlt: Auswahl der passenden Notizen, bevor das Modell gefragt wird —
sonst passt ein Vault nicht in den Kontext.

Erst als Volltextsuche ueber die Titel und Auszuege, die schon da sind. Erst
wenn das zu ungenau ist, eine Einbettungssuche — und dann mit lokalen
Einbettungen, weil sonst der ganze Vault an einen Dienst geht.

### K3 · Werkzeuge

Das Modell soll eine Aufgabe anlegen koennen, nicht nur davon reden. Der
`Connector`-Vertrag hat mit `complete` schon einen schreibenden Aufruf; ein
`create` daneben ist wenig Arbeit. Die Frage ist nicht die Technik, sondern die
Bestaetigung: kein Schreibvorgang ohne sichtbare Rueckfrage.

### K4 · Einordnung statt Formulierung

Der interessantere Einsatz: nicht texten, sondern sortieren. Welche der vier
ungelesenen Mails braucht heute eine Antwort? Welche NocoDB-Zeile ist
tatsaechlich dringend und welche nur alt?

Das setzt voraus, dass sich das Ergebnis pruefen laesst. Deshalb steht es hinten:
eine falsch einsortierte Dringlichkeit ist schlimmer als gar keine.

## Was daneben noch offen ist

### Android Keystore

Der Geraeteschluessel liegt derzeit neben den verschluesselten Daten im privaten
App-Verzeichnis. Das schuetzt gegen andere Apps, nicht gegen jemanden mit
Dateizugriff. Richtig waere ein Schluessel im Android Keystore, der das
Verzeichnis nie verlaesst.

Der Umbau ist klein und lokal — `secrets.rs` hat genau eine Stelle, die den
Schluessel besorgt.

### Widget

Dieselbe Signalzeile auf dem Homescreen. Tauri kann das nicht; es braucht ein
natives Glance-Widget, das die SQLite-Datei mitliest. Deshalb ist die Datenbank
das gemeinsame Format und nicht ein App-interner Zustand.

### Kalender

Auf dem Telefon liegen die Termine schon im System. Ein Connector ueber
`CalendarContract` braucht eine kleine Kotlin-Bruecke — dieselbe Stelle, an der
auch das Widget haengt.

### Wetter statt Wetterfeed

Wetter kommt derzeit als RSS, weil Warnmeldungen so vorliegen. Ein richtiger
Connector (Bright Sky / DWD, kein Schluessel noetig) koennte „Regen ab 14 Uhr"
als Signal statt als Schlagzeile liefern.

### Obsidian schreiben

Tory liest Vaults. Eine Aufgabe abhaken hiesse, eine Zeile in einer Notiz zu
aendern — ueber WebDAV ein `PUT` der ganzen Datei. Machbar, aber es braucht
Sorgfalt: gleichzeitige Aenderungen in Obsidian duerfen nicht verloren gehen.
Solange das nicht sauber geloest ist, bleibt Obsidian lesend.
