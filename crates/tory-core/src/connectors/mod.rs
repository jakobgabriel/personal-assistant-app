//! Der Vertrag, den jede Quelle erfuellt.
//!
//! Eine Quelle anzubinden heisst: [`Connector`] implementieren und in
//! [`crate::sync::build_connectors`] registrieren. Der Startscreen aendert sich
//! dabei nicht.
//!
//! Jeder Connector ist in zwei Haelften geschnitten:
//!
//! * eine reine Abbildungsfunktion (JSON/XML/Markdown -> [`Harvest`]), die ohne
//!   Netz testbar ist,
//! * ein duenner `fetch`, der holt und die Abbildung aufruft.
//!
//! Deshalb liegen in `tests/fixtures/` echte Antworten der Dienste, und die
//! Abbildung ist gegen sie festgenagelt.

pub mod feeds;
pub mod gmail;
pub mod mindwtr;
pub mod nocodb;
pub mod obsidian;

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use reqwest::Client;

use crate::error::Result;
use crate::model::{Harvest, SourceRef, Urgency};
use crate::secrets::SecretStore;

/// Was ein Connector zum Arbeiten braucht. Bewusst schmal: kein Zugriff auf die
/// Datenbank, keine Moeglichkeit, die Oberflaeche anzustossen.
pub struct SyncContext<'a> {
    pub http: &'a Client,
    pub secrets: &'a SecretStore,
    pub now: DateTime<Utc>,
    /// Zeitzone des Nutzers als Offset in Minuten. "Heute" ist lokal, nicht UTC —
    /// sonst kippt die Dringlichkeit um Mitternacht UTC statt um Mitternacht hier.
    pub tz_offset_minutes: i32,
}

impl SyncContext<'_> {
    /// Das lokale Datum zum Zeitpunkt `now`.
    pub fn today(&self) -> NaiveDate {
        (self.now + Duration::minutes(self.tz_offset_minutes as i64)).date_naive()
    }

    /// Wandelt ein lokales Datum in den Zeitpunkt seines Beginns in UTC.
    pub fn local_date_start(&self, date: NaiveDate) -> DateTime<Utc> {
        let naive = date.and_hms_opt(0, 0, 0).unwrap_or_else(|| date.and_hms_opt(0, 0, 0).unwrap());
        Utc.from_utc_datetime(&naive) - Duration::minutes(self.tz_offset_minutes as i64)
    }
}

#[async_trait]
pub trait Connector: Send + Sync {
    fn source(&self) -> &SourceRef;

    /// Holt den aktuellen Stand. Wird nur vom Scheduler gerufen, nie aus der
    /// Oberflaeche.
    async fn fetch(&self, ctx: &SyncContext<'_>) -> Result<Harvest>;

    /// Schreibende Aktion, falls die Quelle eine anbietet (Mindwtr: abhaken).
    /// Standard: die Quelle ist nur lesend.
    async fn complete(&self, _ctx: &SyncContext<'_>, _item_id: &str) -> Result<()> {
        Err(crate::error::Error::other("Diese Quelle ist nur lesend"))
    }
}

/// Dringlichkeit aus einer Faelligkeit. Eine Regel fuer alle Quellen — sonst
/// heisst "heute faellig" bei Mindwtr etwas anderes als bei NocoDB.
pub fn urgency_for_due(due: NaiveDate, today: NaiveDate) -> Urgency {
    match (due - today).num_days() {
        d if d < 0 => Urgency::Critical,
        0 => Urgency::Critical,
        1 => Urgency::High,
        d if d <= 3 => Urgency::Normal,
        _ => Urgency::Info,
    }
}

/// Dringlichkeit aus einem Zeitpunkt mit Uhrzeit (Termine).
pub fn urgency_for_at(at: DateTime<Utc>, now: DateTime<Utc>) -> Urgency {
    let minutes = (at - now).num_minutes();
    match minutes {
        m if m < 0 => Urgency::Normal, // vorbei: kein Alarm mehr, nur Kontext
        m if m <= 120 => Urgency::Critical,
        m if m <= 60 * 12 => Urgency::High,
        m if m <= 60 * 48 => Urgency::Normal,
        _ => Urgency::Info,
    }
}

/// Kuerzt einen Text auf ganze Woerter.
pub fn shorten(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max {
        return text;
    }
    let cut: String = text.chars().take(max).collect();
    match cut.rfind(' ') {
        Some(idx) if idx > max / 2 => format!("{}…", &cut[..idx]),
        _ => format!("{cut}…"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dringlichkeit_aus_faelligkeit() {
        let heute = NaiveDate::from_ymd_opt(2026, 9, 11).unwrap();
        assert_eq!(urgency_for_due(NaiveDate::from_ymd_opt(2026, 9, 10).unwrap(), heute), Urgency::Critical);
        assert_eq!(urgency_for_due(heute, heute), Urgency::Critical);
        assert_eq!(urgency_for_due(NaiveDate::from_ymd_opt(2026, 9, 12).unwrap(), heute), Urgency::High);
        assert_eq!(urgency_for_due(NaiveDate::from_ymd_opt(2026, 9, 14).unwrap(), heute), Urgency::Normal);
        assert_eq!(urgency_for_due(NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(), heute), Urgency::Info);
    }

    #[test]
    fn kuerzen_bricht_an_wortgrenze() {
        assert_eq!(shorten("kurz", 20), "kurz");
        assert_eq!(shorten("ein etwas laengerer Satz hier", 14), "ein etwas…");
        assert_eq!(shorten("mehrere   Leerzeichen", 40), "mehrere Leerzeichen");
    }

    #[test]
    fn heute_richtet_sich_nach_der_zeitzone() {
        let secrets_dir = std::env::temp_dir().join("tory-ctx-test");
        let secrets = SecretStore::open(&secrets_dir).unwrap();
        let http = crate::http::client().unwrap();
        // 22:30 UTC ist in Berlin (UTC+2) bereits der naechste Tag.
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 22, 30, 0).unwrap();
        let ctx = SyncContext { http: &http, secrets: &secrets, now, tz_offset_minutes: 120 };
        assert_eq!(ctx.today(), NaiveDate::from_ymd_opt(2026, 9, 12).unwrap());
        let ctx_utc = SyncContext { http: &http, secrets: &secrets, now, tz_offset_minutes: 0 };
        assert_eq!(ctx_utc.today(), NaiveDate::from_ymd_opt(2026, 9, 11).unwrap());
        let _ = std::fs::remove_dir_all(&secrets_dir);
    }
}
