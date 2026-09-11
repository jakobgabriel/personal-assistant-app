# Archiv

Was hier liegt, gehoert zu einem Entwurf, der nicht weitergefuehrt wird. Es
bleibt stehen, weil die Ueberlegungen darin noch gelten — der Weg dorthin ist
nur ein anderer geworden.

## `android-kotlin/`

Die Schnittstellendefinition als Kotlin-Vertrag, entstanden als Tory noch eine
native Android-App mit Jetpack Compose werden sollte. Dazu ein Skript, das
prueft, ob jede Information aus den Mockups ein Feld im Modell hat.

**Warum abgeloest:** die App ist jetzt eine Tauri-App (Rust + Weboberflaeche).
Ein Kotlin-Vertrag beschreibt damit nichts Gebautes mehr. Der Nachfolger ist
`crates/tory-core/src/model.rs` — dieselben Begriffe, dieselbe Trennung von
Signal und Uebersicht, nur in Rust und mit englischen Bezeichnern.

**Was ebenfalls anders wurde:** der Entwurf ging von Mail, Paketen, Konten,
Aufgaben, Terminen und Nachrichten aus. Tatsaechlich angebunden sind Obsidian,
Mindwtr, NocoDB, Gmail und RSS. Pakete und Bankkonten stehen nicht mehr auf dem
Plan; die Ueberlegungen dazu in `docs/konzept.md` sind entsprechend gekuerzt.
