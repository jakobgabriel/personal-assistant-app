//! Wann welche Quelle abgefragt wird.
//!
//! Kein eigener Hintergrunddienst: die App fragt beim Oeffnen und danach in
//! Abstaenden, was faellig ist. Auf Android stirbt ein langlebiger Dienst unter
//! One UIs Akkuverwaltung ohnehin; das Muster hier ("was ist faellig?") laesst
//! sich sowohl von einem Timer in der App als auch von einem
//! Betriebssystem-Weckruf aus bedienen.
//!
//! Fehlversuche laufen mit exponentiellem Backoff nach, damit eine abgeschaltete
//! NocoDB nicht alle 15 Minuten neu angefragt wird.

use chrono::{DateTime, Duration, Utc};
use reqwest::Client;

use crate::config::Config;
use crate::connectors::{
    feeds::FeedsConnector, gmail::GmailConnector, mindwtr::MindwtrConnector,
    nocodb::NocodbConnector, obsidian::ObsidianConnector, Connector,
};
use crate::model::SyncState;

/// Baut aus der Konfiguration die Liste der aktiven Quellen.
///
/// Ausgeschaltete Quellen tauchen hier nicht auf — sie behalten ihre Daten in
/// der Datenbank, werden aber nicht mehr abgefragt.
pub fn build_connectors(config: &Config) -> Vec<Box<dyn Connector>> {
    let mut out: Vec<Box<dyn Connector>> = Vec::new();
    for s in config.obsidian.iter().filter(|s| s.common.enabled) {
        out.push(Box::new(ObsidianConnector::new(s.clone())));
    }
    for s in config.mindwtr.iter().filter(|s| s.common.enabled) {
        out.push(Box::new(MindwtrConnector::new(s.clone())));
    }
    for s in config.nocodb.iter().filter(|s| s.common.enabled) {
        out.push(Box::new(NocodbConnector::new(s.clone())));
    }
    for s in config.gmail.iter().filter(|s| s.common.enabled) {
        out.push(Box::new(GmailConnector::new(s.clone())));
    }
    for s in config.feeds.iter().filter(|s| s.common.enabled) {
        out.push(Box::new(FeedsConnector::new(s.clone())));
    }
    out
}

/// Der Takt einer Quelle in Minuten, aus der Konfiguration.
pub fn cadence_minutes(config: &Config, source_id: &str) -> Option<u32> {
    let by = |instance: &str, kind: &str| format!("{kind}:{instance}");
    for s in &config.obsidian {
        if by(&s.common.instance, "obsidian") == source_id {
            return Some(s.common.cadence.minutes);
        }
    }
    for s in &config.mindwtr {
        if by(&s.common.instance, "mindwtr") == source_id {
            return Some(s.common.cadence.minutes);
        }
    }
    for s in &config.nocodb {
        if by(&s.common.instance, "nocodb") == source_id {
            return Some(s.common.cadence.minutes);
        }
    }
    for s in &config.gmail {
        if by(&s.common.instance, "gmail") == source_id {
            return Some(s.common.cadence.minutes);
        }
    }
    for s in &config.feeds {
        if by(&s.common.instance, "feeds") == source_id {
            return Some(s.common.cadence.minutes);
        }
    }
    None
}

/// Ist diese Quelle jetzt faellig?
///
/// Noch nie gelaufen -> ja. Sonst: Takt abgelaufen. Nach Fehlversuchen kommt
/// Backoff obendrauf, gedeckelt auf zwei Stunden — laenger zu warten hilft
/// niemandem, der gerade seinen Server repariert hat.
pub fn is_due(state: &SyncState, cadence_minutes: u32, now: DateTime<Utc>) -> bool {
    let Some(last_attempt) = state.last_attempt else {
        return true;
    };
    let base = Duration::minutes(cadence_minutes.max(1) as i64);
    let wait = if state.failures == 0 {
        base
    } else {
        let factor = 2i64.saturating_pow(state.failures.min(6));
        let backoff = Duration::minutes((cadence_minutes.max(1) as i64).saturating_mul(factor));
        backoff.min(Duration::hours(2)).max(base)
    };
    // Ein nicht wiederholbarer Fehler (falsche Konfiguration, Token weg) wird
    // nicht im Takt neu versucht — dafuer gibt es den Knopf in den Einstellungen.
    if let Some(fault) = &state.fault {
        if !fault.retryable() {
            return now - last_attempt >= Duration::hours(1);
        }
    }
    now - last_attempt >= wait
}

/// Welche Quellen jetzt abgefragt werden sollen.
pub fn due_sources(
    config: &Config,
    states: &[SyncState],
    now: DateTime<Utc>,
) -> Vec<String> {
    config
        .sources()
        .into_iter()
        .filter_map(|source| {
            let id = source.id();
            let cadence = cadence_minutes(config, &id)?;
            let state = states
                .iter()
                .find(|s| s.source == id)
                .cloned()
                .unwrap_or_else(|| SyncState::fresh(&id));
            is_due(&state, cadence, now).then_some(id)
        })
        .collect()
}

/// Ein HTTP-Client fuer alle Quellen. Verbindungen werden so wiederverwendet.
pub fn http_client() -> crate::Result<Client> {
    crate::http::client()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Cadence, Feed, FeedTopic, FeedsSource, MindwtrSource, SourceCommon};
    use crate::model::SyncFault;

    fn config() -> Config {
        Config {
            mindwtr: vec![MindwtrSource {
                common: SourceCommon::new("haupt", "Mindwtr", Cadence::minutes(15)),
                access: crate::config::MindwtrAccess::Cloud {
                    base_url: "https://m.example.de".into(),
                    token_key: "mindwtr.haupt.token".into(),
                },
                statuses: vec!["next".into()],
                include_undated: false,
                horizon_days: 7,
            }],
            feeds: vec![FeedsSource {
                common: SourceCommon::new("nachrichten", "Nachrichten", Cadence::minutes(30)),
                feeds: vec![Feed {
                    url: "https://example.de/rss".into(),
                    label: "Beispiel".into(),
                    topic: FeedTopic::Global,
                    enabled: true,
                }],
                headline_limit: 10,
                max_age_hours: 36,
            }],
            ..Config::default()
        }
    }

    #[test]
    fn noch_nie_gelaufen_ist_faellig() {
        assert!(is_due(&SyncState::fresh("x"), 15, Utc::now()));
    }

    #[test]
    fn takt_wird_eingehalten() {
        let now = Utc::now();
        let state = SyncState {
            last_attempt: Some(now - Duration::minutes(10)),
            last_ok: Some(now - Duration::minutes(10)),
            ..SyncState::fresh("x")
        };
        assert!(!is_due(&state, 15, now));
        assert!(is_due(&state, 15, now + Duration::minutes(6)));
    }

    #[test]
    fn fehlversuche_verlangsamen_exponentiell() {
        let now = Utc::now();
        let state = SyncState {
            last_attempt: Some(now - Duration::minutes(20)),
            failures: 3,
            fault: Some(SyncFault::Server { status: 500, detail: String::new() }),
            ..SyncState::fresh("x")
        };
        // 15 min * 2^3 = 120 min
        assert!(!is_due(&state, 15, now), "nach drei Fehlern nicht schon nach 20 Minuten");
        assert!(is_due(&state, 15, now + Duration::hours(2)));
    }

    #[test]
    fn backoff_ist_bei_zwei_stunden_gedeckelt() {
        let now = Utc::now();
        let state = SyncState {
            last_attempt: Some(now - Duration::hours(3)),
            failures: 20,
            fault: Some(SyncFault::Offline),
            ..SyncState::fresh("x")
        };
        assert!(is_due(&state, 15, now), "nach drei Stunden wird wieder versucht");
    }

    #[test]
    fn falsche_konfiguration_wird_nicht_im_takt_wiederholt() {
        let now = Utc::now();
        let state = SyncState {
            last_attempt: Some(now - Duration::minutes(16)),
            failures: 1,
            fault: Some(SyncFault::Misconfigured { detail: "Tabelle fehlt".into() }),
            ..SyncState::fresh("x")
        };
        assert!(!is_due(&state, 15, now), "Konfigurationsfehler heilt nicht durch Warten");
        assert!(is_due(&state, 15, now + Duration::hours(1)));
    }

    #[test]
    fn faellige_quellen_richten_sich_nach_ihrem_takt() {
        let config = config();
        let now = Utc::now();
        let states = vec![
            SyncState { last_attempt: Some(now - Duration::minutes(20)), ..SyncState::fresh("mindwtr:haupt") },
            SyncState { last_attempt: Some(now - Duration::minutes(20)), ..SyncState::fresh("feeds:nachrichten") },
        ];
        let due = due_sources(&config, &states, now);
        assert_eq!(due, vec!["mindwtr:haupt"], "Feeds haben 30 Minuten Takt");
    }

    #[test]
    fn unbekannte_quellen_werden_beim_start_alle_abgefragt() {
        let due = due_sources(&config(), &[], Utc::now());
        assert_eq!(due.len(), 2);
    }

    #[test]
    fn ausgeschaltete_quelle_wird_nicht_gebaut() {
        let mut config = config();
        config.mindwtr[0].common.enabled = false;
        let connectors = build_connectors(&config);
        assert_eq!(connectors.len(), 1);
        assert_eq!(connectors[0].source().id(), "feeds:nachrichten");
    }

    #[test]
    fn takt_wird_pro_quelle_gefunden() {
        let config = config();
        assert_eq!(cadence_minutes(&config, "mindwtr:haupt"), Some(15));
        assert_eq!(cadence_minutes(&config, "feeds:nachrichten"), Some(30));
        assert_eq!(cadence_minutes(&config, "gibt:es:nicht"), None);
    }
}
