package de.tory.core.model

import java.time.LocalDate

/**
 * Was eine Domaene auf dem Startscreen ueber sich sagt - als Karte (Richtung A),
 * als Zeile der Kennzahlenleiste (B) oder als Kachel (C). Eine Struktur fuer alle
 * drei Designrichtungen; die Richtung entscheidet nur, welche Felder sie zeigt.
 */
data class DomaenenUebersicht(
    val quelle: QuellenId,
    /** Bereits formatiert fuer die Anzeige: "4", "3.428,60 EUR", "19 Grad". */
    val kennzahl: String,
    /** Derselbe Wert unformatiert, fuer Schwellen und Sortierung. */
    val kennzahlRoh: Double?,
    /** Was die Kennzahl zaehlt: "ungelesen", "unterwegs", "offen". */
    val bezeichnung: String,
    /** Eine Praezisierung: "1 mit Frist". */
    val zusatz: String?,
    val betrag: Geld?,
    val trend: Trend?,
    val fortschritt: Fortschritt?,
    /** Bis zu drei Zeilen Inhalt fuer Karte oder Kachel. */
    val zeilen: List<Kurzzeile>,
    val stand: SyncStand,
    val aktion: Aktion?,
)

/** Eine Inhaltszeile in Karte, Kachel oder Widget. */
data class Kurzzeile(
    /** Fuehrende Spalte: Uhrzeit, Kategoriefarbe, Kennzeichen. */
    val fuehrend: String?,
    val text: String,
    /** Nachlaufende Spalte: Betrag, Anzahl, relative Zeit. */
    val nachlaufend: String?,
    val betont: Boolean,
    val erledigt: Boolean,
)

/** Verlauf fuer Sparklines und Veraenderungsangaben. */
data class Trend(
    val punkte: List<Verlaufspunkt>,
    val veraenderung: Geld?,
    val veraenderungProzent: Double?,
    val zeitraumTage: Int,
    /** true = steigend ist gut. Steuert die Farbe, nicht die Form. */
    val richtungGut: Boolean?,
)

data class Verlaufspunkt(val datum: LocalDate, val wert: Double)

/** Fuer Budgetbalken und Punktketten: erreicht von ziel, optional mit Sollwert. */
data class Fortschritt(
    val erreicht: Double,
    val ziel: Double,
    /** Wo man am heutigen Tag stehen sollte, als Anteil von 0 bis 1. */
    val sollAnteil: Double?,
    val einheit: String?,
)

/** Der gesamte Startscreen in einem Objekt - das einzige, was die UI liest. */
data class Startseite(
    val begruessung: Begruessung,
    /** Aeltester Stand aller aktiven Quellen. Beantwortet "wie alt sind die Zahlen?". */
    val stand: SyncStand,
    val signale: List<Signal>,
    val termineHeute: List<Termin>,
    val faelligeAufgaben: List<Aufgabe>,
    /** In der vom Nutzer gewaehlten Reihenfolge. */
    val kacheln: List<DomaenenUebersicht>,
    val hinweise: List<Systemhinweis>,
)

data class Begruessung(
    val name: String,
    val datum: LocalDate,
    val abschnitt: Tagesabschnitt,
)

enum class Tagesabschnitt { MORGEN, TAG, ABEND, NACHT }

/** Meldung ueber die App selbst: abgelaufene Anmeldung, Kontingent, Netz. */
data class Systemhinweis(
    val quelle: QuellenId,
    val text: String,
    val schwere: Dringlichkeit,
    val aktion: Aktion?,
)
