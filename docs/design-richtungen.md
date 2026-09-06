# Tory — Drei Designrichtungen

Interaktive Mockups: `mockups/tory-mockups.html` (als Artifact gerendert).
Alle drei zeigen denselben Sonntagmorgen mit denselben Beispieldaten.

## A · Ruhiger Morgen

Hell, luftig, nah an One UI. Große weiche Karten, viel Weißraum, eine Sache pro
Zeile. Farbe kommt fast nur über runde Icon-Kacheln.

* **Palette:** `#F4F2ED` Grund, `#FFFFFF` Karten, `#0F5C63` Akzent, `#BF5340` Warnung, `#1B1E1C` Text
* **Typo:** Instrument Sans, 23/13/11 px, Versalien nur für Labels
* **Dafür:** wirkt wie ein Systemscreen; am schnellsten gebaut; verzeiht unfertige Module
* **Dagegen:** wenig Information pro Blick; helle Fläche früh morgens unangenehm; wenig eigener Charakter
* **Aufwand:** ~2 Wochen

## B · Cockpit

Dunkel, dicht, ohne Kartenrahmen. Haarlinien statt Boxen, schmale Statusbalken
links markieren Dringlichkeit, Zahlen in Monospace mit gleicher Ziffernbreite.
Oben eine Kennzahlenleiste über alle Domänen.

* **Palette:** `#0C0F11` Grund, `#141819` Flächen, `#56B7C8` Akzent, `#E0A44A` Warnung, `#D45B4A` kritisch
* **Typo:** Instrument Sans + IBM Plex Mono für alle Zahlen
* **Dafür:** doppelt so viel Information pro Blick wie A; Dringlichkeit ohne Lesen erkennbar; OLED-sparsam
* **Dagegen:** braucht saubere Daten; wirkt nach Arbeit; heller Modus muss zusätzlich entworfen werden
* **Aufwand:** ~3 Wochen

## C · Bento

Raster aus Kacheln unterschiedlicher Größe, jede Kachel eine Information, jede
Domäne eine Farbe. Anordnung per Halten und Ziehen. Kachelformat = Widgetformat.
Nachrichtenkachel mit Serifenschrift.

* **Palette:** `#EBEDF0` Grund, Domänenfarben `#B85C22` Paket, `#3A4CA0` Mail, `#1B6B4C` Geld, `#63479E` Aufgaben
* **Typo:** Instrument Sans + Newsreader für Schlagzeilen
* **Dafür:** Dashboard selbst umbaubar; Kachel und Widget sind dieselbe Komponente; Farbcodierung
* **Dagegen:** deutlich mehr Arbeit (Raster, Drag & Drop, Größenlogik, Speicherung); Farbdisziplin nötig; statische Reihenfolge verliert die Dringlichkeitssortierung
* **Aufwand:** ~5 Wochen

## Vergleich

| Richtung | Info pro Blick | Charakter | Widget-Nähe | Aufwand |
| --- | --- | --- | --- | --- |
| A · Ruhiger Morgen | 6–8 Zeilen | freundlich, systemnah | mittel | ~2 Wochen |
| B · Cockpit | 14–18 Zeilen | nüchtern, technisch | gut | ~3 Wochen |
| C · Bento | 10–12 Kacheln | verspielt, modular | sehr gut | ~5 Wochen |

## Empfehlung

**B als Basis, A als Tonfall für die Detailseiten.**

Acht Domänen auf einem Screen sind ein Dichteproblem, und genau das löst B:
Statusbalken und Monospace-Zahlen machen den Gesamtstand in zwei Sekunden
lesbar. Detailseiten dürfen danach A folgen — dort geht es um eine Sache, da
hilft Luft. C ist der stärkste Entwurf, aber der teuerste; die Kachelkomponente
lohnt sich später für die Homescreen-Widgets.
