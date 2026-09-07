package de.tory.core.model

import java.time.Duration
import java.time.Instant
import java.time.LocalDate
import java.time.YearMonth

/** Jedes Detailobjekt einer Domaene. Aus Elementen entstehen Signale. */
sealed interface DomaenenElement {
    val id: String
    val quelle: QuellenId
}

// ---------------------------------------------------------------- Mail

data class MailNachricht(
    override val id: String,
    val absenderName: String,
    val absenderAdresse: String,
    /** Zwei Buchstaben fuer den Avatar-Kreis. */
    val absenderKuerzel: String,
    val betreff: String,
    val auszug: String,
    val empfangen: Instant,
    val ungelesen: Boolean,
    val wichtig: Boolean,
    /** Gesetzt, wenn die Mail eine erkannte Antwortfrist enthaelt. */
    val frist: Instant?,
    val buendel: BuendelArt?,
    val threadId: String,
    val anhaenge: Int,
    /** Datum, an dem Tory die Mail selbsttaetig aus dem Fokus nimmt. */
    val autoArchivAm: LocalDate?,
    val aktion: Aktion,
) : DomaenenElement {
    override val quelle: QuellenId get() = QuellenId.MAIL
}

enum class BuendelArt { NEWSLETTER, RECHNUNG, VERSAND, SOZIALES, SONSTIGES }

/** Zusammengefasste Mails, die einzeln keine Aufmerksamkeit verdienen. */
data class Buendel(
    val art: BuendelArt,
    val bezeichnung: String,
    val anzahl: Int,
    val absenderBeispiele: List<String>,
    val neuesteAm: Instant,
)

// ---------------------------------------------------------------- Pakete

data class Sendung(
    override val id: String,
    val dienst: Versanddienst,
    val haendler: String?,
    val sendungsnummer: String,
    val bezeichnung: String,
    val status: SendungsStatus,
    /** Tatsaechlich erreichte Stationen, aelteste zuerst. */
    val stationen: List<Station>,
    /** Erwartete Stationsnamen dieses Dienstes - die Beschriftung der Punktkette. */
    val stationsplan: List<String>,
    val zustellfenster: Zeitfenster?,
    val voraussichtlichAm: LocalDate?,
    val zugestelltAm: Instant?,
    val empfangsart: Empfangsart?,
    val ablageort: String?,
    val istRetoure: Boolean,
    /** Bei Retouren: erwartete oder erhaltene Erstattung. */
    val erstattung: Geld?,
    val nachverfolgungUrl: String?,
) : DomaenenElement {
    override val quelle: QuellenId get() = QuellenId.PAKET
}

enum class Versanddienst { DHL, HERMES, DPD, UPS, GLS, AMAZON, POST, ANDERE }

enum class SendungsStatus {
    ANGEKUENDIGT, ABGEHOLT, IM_TRANSIT, IN_ZUSTELLUNG,
    ABHOLBEREIT, ZUGESTELLT, PROBLEM, RETOURE_UNTERWEGS, RETOURE_ERSTATTET,
}

data class Station(
    val bezeichnung: String,
    val ort: String?,
    val zeitpunkt: Instant,
    val erreicht: Boolean,
)

enum class Empfangsart { PERSOENLICH, NACHBAR, PAKETSHOP, PACKSTATION, ABSTELLORT }

// ---------------------------------------------------------------- Geld

data class Konto(
    override val id: String,
    val bank: String,
    val bezeichnung: String,
    val art: Kontoart,
    /** Nur die letzten vier Stellen - die volle IBAN wird nie gespeichert. */
    val ibanEndung: String,
    val saldo: Geld,
    val verfuegbar: Geld,
    val zinssatz: Double?,
    val verlauf: List<Verlaufspunkt>,
    val aktualisiertAm: Instant,
) : DomaenenElement {
    override val quelle: QuellenId get() = QuellenId.GELD
}

enum class Kontoart { GIRO, TAGESGELD, KREDITKARTE, DEPOT, SPAREN }

data class Umsatz(
    override val id: String,
    val kontoId: String,
    val gegenseite: String,
    val verwendungszweck: String?,
    val betrag: Geld,
    val buchungstag: LocalDate,
    val wertstellung: LocalDate?,
    val kategorie: Kategorie,
    val wiederkehrend: Boolean,
) : DomaenenElement {
    override val quelle: QuellenId get() = QuellenId.GELD
}

data class Kategorie(
    val schluessel: String,
    val name: String,
    /** Punktfarbe als #RRGGBB, aus der Palette der gewaehlten Designrichtung. */
    val farbe: String,
)

/** Bekannte kuenftige Belastung - Dauerauftrag, Lastschrift, Abo. */
data class GeplanteBuchung(
    override val id: String,
    val bezeichnung: String,
    val betrag: Geld,
    val faelligAm: LocalDate,
    val kontoId: String,
    val art: Buchungsart,
    /** Ob der Kontostand die Buchung am Faelligkeitstag traegt. */
    val gedeckt: Boolean,
) : DomaenenElement {
    override val quelle: QuellenId get() = QuellenId.GELD
}

enum class Buchungsart { DAUERAUFTRAG, LASTSCHRIFT, ABO, RATE }

data class Budget(
    val periode: YearMonth,
    val grenze: Geld,
    val verbraucht: Geld,
    /** Anteil des Monats, der vorbei ist - der Sollstrich im Balken. */
    val sollAnteil: Double,
    val proKategorie: Map<String, Geld>,
)

// ---------------------------------------------------------------- Aufgaben und Termine

data class Aufgabe(
    override val id: String,
    val titel: String,
    val notiz: String?,
    val faelligAm: LocalDate?,
    val erledigt: Boolean,
    val erledigtAm: Instant?,
    val liste: String,
    val herkunft: Herkunft?,
    val erinnerung: Instant?,
) : DomaenenElement {
    override val quelle: QuellenId get() = QuellenId.AUFGABE
}

data class Termin(
    override val id: String,
    val titel: String,
    val ort: String?,
    val beginn: Instant,
    val ende: Instant,
    val ganztaegig: Boolean,
    val kalenderName: String,
    /** Geschaetzte Anreise zum Ort, wenn er aufloesbar ist. */
    val reisezeit: Duration?,
    val teilnehmer: List<String>,
    val aktion: Aktion?,
) : DomaenenElement {
    override val quelle: QuellenId get() = QuellenId.KALENDER
}

// ---------------------------------------------------------------- News

data class Schlagzeile(
    override val id: String,
    val titel: String,
    val quellenName: String,
    val veroeffentlicht: Instant,
    val url: String,
    val gelesen: Boolean,
    val themen: List<String>,
) : DomaenenElement {
    override val quelle: QuellenId get() = QuellenId.NEWS
}

/** Der Rest hinter den gezeigten Schlagzeilen: "+ 10 weitere aus 4 Quellen". */
data class NachrichtenLage(
    val anzahlGesamt: Int,
    val anzahlGezeigt: Int,
    val quellenAnzahl: Int,
    val neuesteAm: Instant,
)

// ---------------------------------------------------------------- Puls und Wetter

data class PulsTag(
    override val id: String,
    val datum: LocalDate,
    val schritte: Int,
    val schlafMinuten: Int?,
    val ruhepuls: Int?,
    val trainings: List<Training>,
) : DomaenenElement {
    override val quelle: QuellenId get() = QuellenId.PULS
}

data class Training(val art: String, val dauerMinuten: Int, val beginn: Instant)

data class WetterLage(
    override val id: String,
    val ort: String,
    val temperaturC: Double,
    val gefuehltC: Double?,
    val zustand: Wetterzustand,
    /** Zeitpunkt, ab dem Regen erwartet wird - Kern der Regenwarnung. */
    val regenAb: Instant?,
    val regenWahrscheinlichkeit: Int,
    val warnung: String?,
) : DomaenenElement {
    override val quelle: QuellenId get() = QuellenId.WETTER
}

enum class Wetterzustand { KLAR, BEWOELKT, REGEN, SCHNEE, NEBEL, GEWITTER }
