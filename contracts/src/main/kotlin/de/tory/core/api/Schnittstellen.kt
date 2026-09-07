package de.tory.core.api

import de.tory.core.model.Aktion
import de.tory.core.model.AnmeldeErgebnis
import de.tory.core.model.DomaenenElement
import de.tory.core.model.DomaenenUebersicht
import de.tory.core.model.Kurzzeile
import de.tory.core.model.QuellenId
import de.tory.core.model.QuellenStatus
import de.tory.core.model.Signal
import de.tory.core.model.Startseite
import de.tory.core.model.SyncAnlass
import de.tory.core.model.SyncErgebnis
import de.tory.core.model.SyncStand
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import java.time.Instant

/**
 * Der Vertrag, den jedes :feature-Modul erfuellt. Ein neues Modul anzubinden
 * heisst: diese Schnittstelle implementieren und registrieren - der
 * Startscreen aendert sich dabei nicht.
 */
interface DomaeneModul {
    val id: QuellenId

    /** Was auf den Startscreen darf. Leer, wenn gerade nichts ansteht. */
    fun signale(): Flow<List<Signal>>

    /** Die Karte, Zeile oder Kachel dieser Domaene auf dem Startscreen. */
    fun uebersicht(): Flow<DomaenenUebersicht>

    /** Die Detailseite. */
    fun elemente(filter: Filter = Filter.KEINER): Flow<List<DomaenenElement>>

    fun status(): StateFlow<QuellenStatus>

    /** Wird vom WorkManager gerufen, nie aus der Oberflaeche. */
    suspend fun sync(anlass: SyncAnlass): SyncErgebnis

    suspend fun anmelden(): AnmeldeErgebnis

    /** Loescht Tokens und alle lokalen Daten dieser Quelle. */
    suspend fun abmelden()
}

data class Filter(
    val nurOffen: Boolean = false,
    val seit: Instant? = null,
    val suche: String? = null,
    val grenze: Int = 50,
) {
    companion object {
        val KEINER = Filter()
    }
}

/** Die einzige Quelle, aus der der Startscreen liest. */
interface DashboardRepository {
    fun startseite(): Flow<Startseite>

    /** Quellenuebergreifend sortiert, standardmaessig die drei wichtigsten. */
    fun signale(grenze: Int = 3): Flow<List<Signal>>

    suspend fun aktualisiereAlles(anlass: SyncAnlass): List<SyncErgebnis>

    suspend fun fuehreAus(aktion: Aktion)

    suspend fun stummschalten(signalId: String, bis: Instant)
}

/** Homescreen-Widgets lesen dieselben Daten wie die App, nur kuerzer. */
interface WidgetRepository {
    fun inhalt(groesse: Widgetgroesse): Flow<WidgetInhalt>
}

enum class Widgetgroesse { KLEIN_4X1, MITTEL_4X2, GROSS_4X4 }

data class WidgetInhalt(
    val zeilen: List<Kurzzeile>,
    val stand: SyncStand,
    /** Fertig formatiert, weil Glance keine Formatierlogik ausfuehren soll. */
    val standText: String,
)

interface Einstellungen {
    fun kachelOrdnung(): Flow<List<Kachelplatz>>
    suspend fun setzeKachelOrdnung(plaetze: List<Kachelplatz>)

    fun aktiveQuellen(): Flow<Set<QuellenId>>
    suspend fun setzeQuelleAktiv(quelle: QuellenId, aktiv: Boolean)

    fun designrichtung(): Flow<Designrichtung>
    fun nutzername(): Flow<String>

    /** Uhrzeit des Morgenbriefs, als Minuten seit Mitternacht. */
    fun morgenbriefUm(): Flow<Int?>
}

data class Kachelplatz(
    val quelle: QuellenId,
    val position: Int,
    val groesse: Kachelgroesse,
)

enum class Kachelgroesse { EINS_MAL_EINS, ZWEI_MAL_EINS, ZWEI_MAL_ZWEI }

enum class Designrichtung { RUHIGER_MORGEN, COCKPIT, BENTO }
