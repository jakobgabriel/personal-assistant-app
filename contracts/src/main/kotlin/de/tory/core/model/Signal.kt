package de.tory.core.model

import java.time.Instant

/**
 * Der gemeinsame Nenner aller Domaenen.
 *
 * Alles, was auf dem Startscreen erscheinen darf, ist ein Signal: etwas mit
 * Zeitpunkt, Betrag oder Handlungsbedarf. Der Startscreen kennt nur diesen Typ
 * und sortiert nach [dringlichkeit], dann nach der Naehe von [zeitpunkt] zu jetzt.
 */
data class Signal(
    val id: String,
    val quelle: QuellenId,
    /** Zeigt auf das Element in der Domaene, aus dem das Signal entstanden ist. */
    val elementId: String,
    /** Eine Zeile, ohne Praefix: "DHL kommt 14-16 Uhr". */
    val titel: String,
    /** Kontext in der zweiten Zeile: "Kopfhoerer - persoenlich". */
    val untertitel: String?,
    val zeitpunkt: Instant?,
    /** Gesetzt, wenn statt eines Zeitpunkts eine Spanne gilt (Zustellfenster). */
    val zeitfenster: Zeitfenster?,
    /** Bestimmt die Formulierung: "in 5 h" vs. "Frist" vs. "faellig am". */
    val zeitpunktArt: ZeitpunktArt?,
    val dringlichkeit: Dringlichkeit,
    val betrag: Geld?,
    /** Fachliche Kennung zum Nachschlagen: Sendungsnummer, IBAN-Endung, Ort. */
    val kennung: String?,
    val aktion: Aktion?,
    /** Ob das Signal direkt abgehakt werden kann (Aufgaben, gelesene Mails). */
    val erledigbar: Boolean,
    /** Vom Nutzer stummgeschaltet bis zu diesem Zeitpunkt. */
    val stummBis: Instant?,
)

/** Alle Domaenen, die Signale liefern koennen. Eine je Modul. */
enum class QuellenId { MAIL, PAKET, GELD, KALENDER, AUFGABE, NEWS, PULS, WETTER, WEG }

/** Steuert Sortierung und Farbe des Statusbalkens. */
enum class Dringlichkeit { KRITISCH, HOCH, NORMAL, INFO }

enum class ZeitpunktArt { TERMIN, FRIST, ZUSTELLUNG, FAELLIGKEIT, VEROEFFENTLICHT }

data class Zeitfenster(val von: Instant, val bis: Instant)

/** Betrag in der kleinsten Waehrungseinheit - nie Double fuer Geld. */
data class Geld(
    val cent: Long,
    val waehrung: String = "EUR",
)

/** Was beim Antippen passiert. Tory handelt nie selbst, es delegiert. */
sealed interface Aktion {
    data class OeffneApp(val paket: String, val deepLink: String?) : Aktion
    data class OeffneUrl(val url: String) : Aktion
    data class OeffneDetail(val quelle: QuellenId, val elementId: String) : Aktion
    data class ErledigeAufgabe(val aufgabeId: String) : Aktion
    data class ErzeugeAufgabe(val titel: String, val herkunft: Herkunft) : Aktion
    data class MarkiereGelesen(val elementId: String) : Aktion
}

/** Woher eine Aufgabe stammt, wenn sie aus einer Mail oder Sendung entstand. */
data class Herkunft(
    val quelle: QuellenId,
    val elementId: String,
    val bezeichnung: String,
)
