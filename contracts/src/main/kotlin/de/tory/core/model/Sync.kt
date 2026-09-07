package de.tory.core.model

import java.time.Instant

/**
 * Der Aktualitaetsstand einer Quelle. Wird ueberall mitgefuehrt, damit die
 * Oberflaeche nie alte Zahlen als aktuell ausgibt.
 */
data class SyncStand(
    val quelle: QuellenId?,
    val letzterErfolg: Instant?,
    val letzterVersuch: Instant?,
    val naechsterVersuch: Instant?,
    val fehler: SyncFehler?,
)

sealed interface SyncFehler {
    val meldung: String

    data class KeinNetz(override val meldung: String) : SyncFehler
    /** Braucht eine Nutzeraktion: OAuth abgelaufen, PSD2-Reconsent nach 90 Tagen. */
    data class AnmeldungAbgelaufen(override val meldung: String, val erneuernUrl: String?) : SyncFehler
    data class KontingentErschoepft(override val meldung: String, val wiederAb: Instant?) : SyncFehler
    data class Serverfehler(override val meldung: String, val code: Int) : SyncFehler
    data class Unbekannt(override val meldung: String) : SyncFehler
}

enum class SyncAnlass { PLANMAESSIG, APP_START, NUTZER, BENACHRICHTIGUNG }

data class SyncErgebnis(
    val quelle: QuellenId,
    val neu: Int,
    val geaendert: Int,
    val entfernt: Int,
    val dauerMs: Long,
    val fehler: SyncFehler?,
)

data class QuellenStatus(
    val quelle: QuellenId,
    val aktiv: Boolean,
    val angemeldet: Boolean,
    val stand: SyncStand,
    val taktMinuten: Int,
)

data class AnmeldeErgebnis(val erfolgreich: Boolean, val fehler: SyncFehler?)
