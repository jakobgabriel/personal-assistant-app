//! SQLite als einzige Wahrheit auf dem Geraet.
//!
//! Die Oberflaeche liest **nie** direkt vom Netz. Sie liest hier, und hier steht
//! der letzte erfolgreiche Stand jeder Quelle. Daraus folgt: die App ist beim
//! Start sofort da, ein Sync im Hintergrund tauscht die Zeilen aus, und ein
//! Ausfall einer Quelle laesst die anderen Karten unberuehrt.
//!
//! Signale werden pro Quelle komplett ersetzt (`replace_signals`), nicht
//! gemergt. Eine Quelle ist die Wahrheit ueber ihre eigenen Signale; was sie
//! nicht mehr liefert, ist weg. Lokale Zusaetze — stummgeschaltet bis,
//! abgehakt — liegen deshalb in einer eigenen Tabelle, die das Ersetzen
//! ueberlebt.

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;
use crate::model::{Harvest, Overview, Signal, SyncFault, SyncState};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Fuer Tests und fuer den ersten Start ohne Schreibrechte.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS signals (
                source      TEXT NOT NULL,
                id          TEXT NOT NULL,
                payload     TEXT NOT NULL,
                urgency     INTEGER NOT NULL,
                at          TEXT,
                dedup_key   TEXT,
                PRIMARY KEY (source, id)
            );
            CREATE INDEX IF NOT EXISTS signals_sort ON signals (urgency, at);

            CREATE TABLE IF NOT EXISTS overviews (
                source   TEXT PRIMARY KEY,
                payload  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sync_state (
                source        TEXT PRIMARY KEY,
                last_ok       TEXT,
                last_attempt  TEXT,
                fault         TEXT,
                failures      INTEGER NOT NULL DEFAULT 0
            );

            -- Ueberlebt das Ersetzen der Signale einer Quelle.
            CREATE TABLE IF NOT EXISTS local_marks (
                key         TEXT PRIMARY KEY,
                muted_until TEXT,
                done_at     TEXT
            );
            "#,
        )?;
        Ok(Self { conn })
    }

    /// Ersetzt Signale und Uebersicht einer Quelle in einer Transaktion — die
    /// Oberflaeche sieht nie einen halben Stand.
    pub fn apply_harvest(&mut self, harvest: &Harvest) -> Result<()> {
        let source = harvest.source.id();
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM signals WHERE source = ?1", params![source])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO signals (source, id, payload, urgency, at, dedup_key)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for signal in &harvest.signals {
                insert.execute(params![
                    source,
                    signal.id,
                    serde_json::to_string(signal)?,
                    signal.urgency.rank() as i64,
                    signal.at.map(|t| t.to_rfc3339()),
                    signal.dedup_key,
                ])?;
            }
        }
        tx.execute(
            "INSERT INTO overviews (source, payload) VALUES (?1, ?2)
             ON CONFLICT(source) DO UPDATE SET payload = excluded.payload",
            params![source, serde_json::to_string(&harvest.overview)?],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Alle Signale, sortiert wie der Startscreen sie zeigt: Dringlichkeit,
    /// dann Naehe des Zeitpunkts. Lokal Abgehaktes und Stummgeschaltetes ist
    /// heraus, doppelte `dedup_key` erscheinen einmal.
    pub fn signals(&self, now: DateTime<Utc>) -> Result<Vec<Signal>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.payload
               FROM signals s
               LEFT JOIN local_marks m ON m.key = s.source || '/' || s.id
              WHERE m.done_at IS NULL
                AND (m.muted_until IS NULL OR m.muted_until < ?1)
              ORDER BY s.urgency ASC, s.at IS NULL ASC, s.at ASC",
        )?;
        let rows = stmt.query_map(params![now.to_rfc3339()], |row| row.get::<_, String>(0))?;
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for row in rows {
            let signal: Signal = serde_json::from_str(&row?)?;
            if let Some(key) = &signal.dedup_key {
                if !seen.insert(key.clone()) {
                    continue;
                }
            }
            out.push(signal);
        }
        Ok(out)
    }

    pub fn signals_of(&self, source: &str, now: DateTime<Utc>) -> Result<Vec<Signal>> {
        Ok(self.signals(now)?.into_iter().filter(|s| s.source.id() == source).collect())
    }

    pub fn overviews(&self) -> Result<Vec<Overview>> {
        let mut stmt = self.conn.prepare("SELECT payload FROM overviews")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    pub fn sync_state(&self, source: &str) -> Result<SyncState> {
        let row = self
            .conn
            .query_row(
                "SELECT last_ok, last_attempt, fault, failures FROM sync_state WHERE source = ?1",
                params![source],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((last_ok, last_attempt, fault, failures)) = row else {
            return Ok(SyncState::fresh(source));
        };
        Ok(SyncState {
            source: source.to_string(),
            last_ok: parse_time(last_ok),
            last_attempt: parse_time(last_attempt),
            fault: fault.and_then(|f| serde_json::from_str(&f).ok()),
            failures: failures as u32,
        })
    }

    pub fn sync_states(&self) -> Result<Vec<SyncState>> {
        let mut stmt = self.conn.prepare("SELECT source FROM sync_state")?;
        let sources: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<_>>()?;
        sources.iter().map(|s| self.sync_state(s)).collect()
    }

    /// Haelt fest, dass ein Sync gelaufen ist. `fault = None` heisst Erfolg und
    /// setzt den Fehlerzaehler zurueck.
    pub fn record_sync(&self, source: &str, at: DateTime<Utc>, fault: Option<&SyncFault>) -> Result<()> {
        match fault {
            None => self.conn.execute(
                "INSERT INTO sync_state (source, last_ok, last_attempt, fault, failures)
                 VALUES (?1, ?2, ?2, NULL, 0)
                 ON CONFLICT(source) DO UPDATE SET
                     last_ok = ?2, last_attempt = ?2, fault = NULL, failures = 0",
                params![source, at.to_rfc3339()],
            )?,
            Some(f) => self.conn.execute(
                "INSERT INTO sync_state (source, last_ok, last_attempt, fault, failures)
                 VALUES (?1, NULL, ?2, ?3, 1)
                 ON CONFLICT(source) DO UPDATE SET
                     last_attempt = ?2, fault = ?3, failures = sync_state.failures + 1",
                params![source, at.to_rfc3339(), serde_json::to_string(f)?],
            )?,
        };
        Ok(())
    }

    /// "Spaeter" in der Signalzeile.
    pub fn mute(&self, key: &str, until: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO local_marks (key, muted_until) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET muted_until = excluded.muted_until",
            params![key, until.to_rfc3339()],
        )?;
        Ok(())
    }

    /// Lokal abgehakt. Bei Mindwtr geschieht zusaetzlich der Aufruf nach
    /// aussen; hier steht nur, dass Tory es nicht mehr zeigt.
    pub fn mark_done(&self, key: &str, at: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO local_marks (key, done_at) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET done_at = excluded.done_at",
            params![key, at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn clear_mark(&self, key: &str) -> Result<()> {
        self.conn.execute("DELETE FROM local_marks WHERE key = ?1", params![key])?;
        Ok(())
    }

    /// Marken, deren Signal es nicht mehr gibt, wachsen sonst unbegrenzt.
    pub fn prune_marks(&self, now: DateTime<Utc>) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM local_marks
              WHERE (muted_until IS NOT NULL AND muted_until < ?1 AND done_at IS NULL)
                 OR key NOT IN (SELECT source || '/' || id FROM signals)",
            params![now.to_rfc3339()],
        )?;
        Ok(n)
    }

    /// Alles einer Quelle loeschen — "Quelle abmelden" in den Einstellungen.
    pub fn forget_source(&self, source: &str) -> Result<()> {
        self.conn.execute("DELETE FROM signals WHERE source = ?1", params![source])?;
        self.conn.execute("DELETE FROM overviews WHERE source = ?1", params![source])?;
        self.conn.execute("DELETE FROM sync_state WHERE source = ?1", params![source])?;
        self.conn
            .execute("DELETE FROM local_marks WHERE key LIKE ?1", params![format!("{source}/%")])?;
        Ok(())
    }
}

fn parse_time(raw: Option<String>) -> Option<DateTime<Utc>> {
    raw.and_then(|s| DateTime::parse_from_rfc3339(&s).ok()).map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SourceKind, SourceRef, TimeKind, Urgency};
    use chrono::Duration;

    fn quelle() -> SourceRef {
        SourceRef::new(SourceKind::Mindwtr, "haupt", "Mindwtr")
    }

    fn signal(id: &str, urgency: Urgency, at: Option<DateTime<Utc>>) -> Signal {
        Signal { urgency, at, time_kind: at.map(|_| TimeKind::Due), ..Signal::new(quelle(), id, id) }
    }

    fn ernte(signals: Vec<Signal>) -> Harvest {
        Harvest { source: quelle(), signals, overview: Overview::empty(quelle(), "Aufgaben") }
    }

    #[test]
    fn sortiert_nach_dringlichkeit_dann_zeit() {
        let now = Utc::now();
        let mut store = Store::open_in_memory().unwrap();
        store
            .apply_harvest(&ernte(vec![
                signal("c", Urgency::Normal, Some(now + Duration::hours(1))),
                signal("a", Urgency::Critical, Some(now + Duration::hours(5))),
                signal("b", Urgency::Critical, Some(now + Duration::hours(2))),
                signal("d", Urgency::Info, None),
            ]))
            .unwrap();
        let ids: Vec<String> = store.signals(now).unwrap().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["b", "a", "c", "d"]);
    }

    #[test]
    fn ersetzt_statt_zu_mergen() {
        let mut store = Store::open_in_memory().unwrap();
        store.apply_harvest(&ernte(vec![signal("alt", Urgency::Normal, None)])).unwrap();
        store.apply_harvest(&ernte(vec![signal("neu", Urgency::Normal, None)])).unwrap();
        let ids: Vec<String> = store.signals(Utc::now()).unwrap().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["neu"]);
    }

    #[test]
    fn lokale_marken_ueberleben_den_sync() {
        let now = Utc::now();
        let mut store = Store::open_in_memory().unwrap();
        let s = signal("x", Urgency::High, None);
        store.apply_harvest(&ernte(vec![s.clone()])).unwrap();
        store.mark_done(&s.key(), now).unwrap();
        assert!(store.signals(now).unwrap().is_empty());

        // Quelle liefert dasselbe Signal erneut — es bleibt abgehakt.
        store.apply_harvest(&ernte(vec![s.clone()])).unwrap();
        assert!(store.signals(now).unwrap().is_empty());

        store.clear_mark(&s.key()).unwrap();
        assert_eq!(store.signals(now).unwrap().len(), 1);
    }

    #[test]
    fn stummschaltung_laeuft_ab() {
        let now = Utc::now();
        let mut store = Store::open_in_memory().unwrap();
        let s = signal("x", Urgency::High, None);
        store.apply_harvest(&ernte(vec![s.clone()])).unwrap();
        store.mute(&s.key(), now + Duration::hours(3)).unwrap();
        assert!(store.signals(now).unwrap().is_empty());
        assert_eq!(store.signals(now + Duration::hours(4)).unwrap().len(), 1);
    }

    #[test]
    fn dedup_key_faltet_doppelte_zusammen() {
        let mut store = Store::open_in_memory().unwrap();
        let mut a = signal("a", Urgency::Normal, None);
        let mut b = signal("b", Urgency::Normal, None);
        a.dedup_key = Some("gleiche-sache".into());
        b.dedup_key = Some("gleiche-sache".into());
        store.apply_harvest(&ernte(vec![a, b])).unwrap();
        assert_eq!(store.signals(Utc::now()).unwrap().len(), 1);
    }

    #[test]
    fn sync_stand_zaehlt_fehler_und_setzt_zurueck() {
        let now = Utc::now();
        let store = Store::open_in_memory().unwrap();
        let fault = SyncFault::Server { status: 502, detail: "bad gateway".into() };
        store.record_sync("mindwtr:haupt", now, Some(&fault)).unwrap();
        store.record_sync("mindwtr:haupt", now, Some(&fault)).unwrap();
        let state = store.sync_state("mindwtr:haupt").unwrap();
        assert_eq!(state.failures, 2);
        assert!(state.last_ok.is_none());

        store.record_sync("mindwtr:haupt", now, None).unwrap();
        let state = store.sync_state("mindwtr:haupt").unwrap();
        assert_eq!(state.failures, 0);
        assert!(state.fault.is_none());
        assert!(state.last_ok.is_some(), "Erfolg muss last_ok setzen");
    }

    #[test]
    fn quelle_vergessen_loescht_alles() {
        let now = Utc::now();
        let mut store = Store::open_in_memory().unwrap();
        let s = signal("x", Urgency::High, None);
        store.apply_harvest(&ernte(vec![s.clone()])).unwrap();
        store.mute(&s.key(), now + Duration::hours(1)).unwrap();
        store.record_sync("mindwtr:haupt", now, None).unwrap();

        store.forget_source("mindwtr:haupt").unwrap();
        assert!(store.signals(now).unwrap().is_empty());
        assert!(store.overviews().unwrap().is_empty());
        assert!(store.sync_states().unwrap().is_empty());
    }
}
