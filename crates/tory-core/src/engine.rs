//! Die eine Tuer, durch die die Oberflaeche geht.
//!
//! Alles, was die Tauri-Schale kann, kann sie ueber [`Engine`]. Damit hat die
//! App genau eine Stelle, an der Konfiguration, Geheimnisse, Datenbank und Netz
//! zusammenkommen — und alles darunter bleibt ohne Tauri testbar.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::ai;
use crate::config::Config;
use crate::connectors::{Connector, SyncContext};
use crate::error::{Error, Result};
use crate::model::{Action, Overview, Signal, SourceRef, SyncState};
use crate::secrets::SecretStore;
use crate::store::Store;
use crate::sync;

/// Was der Startscreen braucht — in einem Aufruf, damit die Oberflaeche nicht
/// fuenf Ladezustaende jonglieren muss.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dashboard {
    pub now: DateTime<Utc>,
    /// Die obersten Signale, nach Dringlichkeit und Zeit.
    pub top: Vec<Signal>,
    /// Alle uebrigen Signale in derselben Sortierung.
    pub rest: Vec<Signal>,
    /// Karten in der konfigurierten Reihenfolge.
    pub cards: Vec<Card>,
    /// Fehler, die ein Banner verdienen.
    pub alerts: Vec<Alert>,
}

/// Eine Domaenenkarte samt Aktualitaet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub source: SourceRef,
    /// `None`, solange die Quelle noch nie erfolgreich synchronisiert hat.
    pub overview: Option<Overview>,
    pub sync: SyncState,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub source: SourceRef,
    pub message: String,
    /// Der Nutzer muss handeln (neu anmelden, Konfiguration richten).
    pub needs_action: bool,
}

/// Ergebnis eines Syncs, fuer die Rueckmeldung in der Oberflaeche.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReport {
    pub synced: Vec<String>,
    pub failed: Vec<SyncFailure>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFailure {
    pub source: String,
    pub message: String,
}

/// Wohin Konfiguration, Geheimnisse und Datenbank gehoeren.
pub struct Paths {
    /// Privates Datenverzeichnis der App.
    pub data_dir: PathBuf,
}

impl Paths {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self { data_dir: data_dir.into() }
    }

    pub fn config_file(&self) -> PathBuf {
        self.data_dir.join("config.json")
    }

    pub fn db_file(&self) -> PathBuf {
        self.data_dir.join("tory.sqlite3")
    }

    pub fn secrets_dir(&self) -> PathBuf {
        self.data_dir.join("secrets")
    }
}

pub struct Engine {
    paths: Paths,
    config: Mutex<Config>,
    store: Mutex<Store>,
    secrets: Mutex<SecretStore>,
    http: reqwest::Client,
    /// Laufende Anmeldeversuche: `instance` -> PKCE-Paar.
    pending_auth: Mutex<Vec<(String, crate::connectors::gmail::AuthStart)>>,
    /// Zeitzonenversatz in Minuten, von der Oberflaeche gesetzt.
    tz_offset: Mutex<i32>,
}

impl Engine {
    /// Oeffnet alles im angegebenen Datenverzeichnis. Fehlt eine Konfiguration,
    /// entsteht eine leere — die App startet dann in die Einstellungen statt mit
    /// einem Fehler.
    pub fn open(data_dir: impl Into<PathBuf>) -> Result<Arc<Self>> {
        let paths = Paths::new(data_dir);
        std::fs::create_dir_all(&paths.data_dir)?;
        let config = load_config(&paths.config_file())?;
        let store = Store::open(&paths.db_file())?;
        let secrets = SecretStore::open(&paths.secrets_dir())?;
        Ok(Arc::new(Self {
            config: Mutex::new(config),
            store: Mutex::new(store),
            secrets: Mutex::new(secrets),
            http: sync::http_client()?,
            pending_auth: Mutex::new(Vec::new()),
            tz_offset: Mutex::new(0),
            paths,
        }))
    }

    /// Die Oberflaeche kennt die Zeitzone des Geraets, der Kern nicht.
    pub async fn set_tz_offset(&self, minutes: i32) {
        *self.tz_offset.lock().await = minutes.clamp(-14 * 60, 14 * 60);
    }

    pub async fn config(&self) -> Config {
        self.config.lock().await.clone()
    }

    /// Schreibt die Konfiguration und raeumt Geheimnisse auf, auf die nichts
    /// mehr verweist.
    pub async fn save_config(&self, new: Config) -> Result<Vec<String>> {
        let problems = new.validate();
        if !problems.is_empty() {
            return Ok(problems);
        }
        let json = serde_json::to_string_pretty(&new)?;
        std::fs::write(self.paths.config_file(), json)?;

        // Quellen, die es nicht mehr gibt, hinterlassen sonst Karten und Zeilen.
        let alt = self.config.lock().await.clone();
        let neue_ids: Vec<String> = new.sources().iter().map(|s| s.id()).collect();
        {
            let store = self.store.lock().await;
            for old in alt.sources() {
                if !neue_ids.contains(&old.id()) {
                    store.forget_source(&old.id())?;
                }
            }
        }
        let keep = new.referenced_secret_keys();
        self.secrets.lock().await.retain_only(&keep)?;
        *self.config.lock().await = new;
        Ok(Vec::new())
    }

    /// Legt ein Geheimnis ab. Der Wert verlaesst die App nie wieder.
    pub async fn set_secret(&self, name: &str, value: &str) -> Result<()> {
        self.secrets.lock().await.set(name, value)
    }

    /// Welche Geheimnisse hinterlegt sind — Namen, keine Werte.
    pub async fn secret_names(&self) -> Vec<String> {
        self.secrets.lock().await.names()
    }

    /// Alles fuer den Startscreen, aus der Datenbank. Kein Netz.
    pub async fn dashboard(&self) -> Result<Dashboard> {
        let now = Utc::now();
        let config = self.config.lock().await.clone();
        let store = self.store.lock().await;

        let mut signals = store.signals(now)?;
        let overviews = store.overviews()?;
        let states = store.sync_states()?;

        let top_count = config.dashboard.top_signals.min(signals.len());
        let top: Vec<Signal> = signals.drain(..top_count).collect();

        let order = &config.dashboard.card_order;
        let mut cards: Vec<Card> = config
            .sources()
            .into_iter()
            .map(|source| {
                let id = source.id();
                Card {
                    overview: overviews.iter().find(|o| o.source.id() == id).cloned(),
                    sync: states
                        .iter()
                        .find(|s| s.source == id)
                        .cloned()
                        .unwrap_or_else(|| SyncState::fresh(&id)),
                    enabled: true,
                    source,
                }
            })
            .collect();
        if !order.is_empty() {
            cards.sort_by_key(|c| {
                order.iter().position(|id| *id == c.source.id()).unwrap_or(usize::MAX)
            });
        }

        let alerts = cards
            .iter()
            .filter_map(|card| {
                let fault = card.sync.fault.as_ref()?;
                // Kein Netz ist kein Banner — das sieht man am Sync-Stand.
                if matches!(fault, crate::model::SyncFault::Offline) {
                    return None;
                }
                Some(Alert {
                    source: card.source.clone(),
                    message: fault.message(),
                    needs_action: !fault.retryable(),
                })
            })
            .collect();

        Ok(Dashboard { now, top, rest: signals, cards, alerts })
    }

    /// Alle Signale einer Quelle — die Detailseite.
    pub async fn signals_of(&self, source_id: &str) -> Result<Vec<Signal>> {
        self.store.lock().await.signals_of(source_id, Utc::now())
    }

    /// Synchronisiert. `force = false` fragt nur, was nach Takt und Backoff
    /// faellig ist; `force = true` alles (der Knopf "jetzt aktualisieren").
    pub async fn sync(&self, force: bool) -> Result<SyncReport> {
        let config = self.config.lock().await.clone();
        let now = Utc::now();
        let tz_offset = *self.tz_offset.lock().await;

        let states = self.store.lock().await.sync_states()?;
        let due = if force {
            config.sources().iter().map(|s| s.id()).collect::<Vec<_>>()
        } else {
            sync::due_sources(&config, &states, now)
        };

        let mut report = SyncReport { synced: Vec::new(), failed: Vec::new(), skipped: Vec::new() };
        for connector in sync::build_connectors(&config) {
            let id = connector.source().id();
            if !due.contains(&id) {
                report.skipped.push(id);
                continue;
            }
            match self.run_one(connector.as_ref(), now, tz_offset).await {
                Ok(()) => report.synced.push(id),
                Err(err) => {
                    let fault = err.as_fault();
                    self.store.lock().await.record_sync(&id, now, Some(&fault))?;
                    report.failed.push(SyncFailure { source: id, message: fault.message() });
                }
            }
        }

        // Marken ohne Signal und abgelaufene Stummschaltungen abraeumen.
        self.store.lock().await.prune_marks(now)?;
        Ok(report)
    }

    async fn run_one(
        &self,
        connector: &dyn Connector,
        now: DateTime<Utc>,
        tz_offset: i32,
    ) -> Result<()> {
        let harvest = {
            let secrets = self.secrets.lock().await;
            let ctx = SyncContext {
                http: &self.http,
                secrets: &secrets,
                now,
                tz_offset_minutes: tz_offset,
            };
            connector.fetch(&ctx).await?
        };
        let mut store = self.store.lock().await;
        store.apply_harvest(&harvest)?;
        store.record_sync(&connector.source().id(), now, None)?;
        Ok(())
    }

    /// Fuehrt die Aktion eines Signals aus, soweit sie in Tory gehoert. URLs und
    /// `obsidian://`-Spruenge gibt die Schale ans Betriebssystem.
    pub async fn act(&self, action: &Action) -> Result<Option<String>> {
        match action {
            Action::OpenUrl { url } => Ok(Some(url.clone())),
            Action::OpenNote { vault, path } => Ok(Some(format!(
                "obsidian://open?vault={}&file={}",
                urlencode(vault),
                urlencode(path)
            ))),
            Action::CompleteTask { source, task_id } => {
                self.complete_task(source, task_id).await?;
                Ok(None)
            }
            Action::ShowNote { .. } | Action::OpenDetail { .. } => Ok(None),
        }
    }

    /// Hakt eine Aufgabe ab: erst in der Quelle, dann lokal. Andersherum saehe
    /// die Aufgabe kurz erledigt aus und waere beim naechsten Sync wieder da.
    pub async fn complete_task(&self, source_id: &str, task_id: &str) -> Result<()> {
        let config = self.config.lock().await.clone();
        let now = Utc::now();
        let tz_offset = *self.tz_offset.lock().await;
        let connector = sync::build_connectors(&config)
            .into_iter()
            .find(|c| c.source().id() == source_id)
            .ok_or_else(|| Error::config(format!("Quelle {source_id} ist nicht aktiv")))?;
        {
            let secrets = self.secrets.lock().await;
            let ctx = SyncContext {
                http: &self.http,
                secrets: &secrets,
                now,
                tz_offset_minutes: tz_offset,
            };
            connector.complete(&ctx, task_id).await?;
        }
        self.store.lock().await.mark_done(&format!("{source_id}/{task_id}"), now)?;
        Ok(())
    }

    /// "Spaeter" in der Signalzeile.
    pub async fn snooze(&self, signal_key: &str, hours: i64) -> Result<()> {
        self.store
            .lock()
            .await
            .mute(signal_key, Utc::now() + Duration::hours(hours.clamp(1, 24 * 30)))
    }

    /// Lokal abhaken, ohne die Quelle anzufassen.
    pub async fn dismiss(&self, signal_key: &str) -> Result<()> {
        self.store.lock().await.mark_done(signal_key, Utc::now())
    }

    /// Beginnt die Gmail-Anmeldung und gibt die URL zurueck, die die Schale im
    /// Systembrowser oeffnet.
    pub async fn begin_gmail_auth(&self, instance: &str) -> Result<String> {
        let config = self.config.lock().await.clone();
        let source = config
            .gmail
            .iter()
            .find(|s| s.common.instance == instance)
            .ok_or_else(|| Error::config(format!("Kein Gmail-Konto '{instance}' konfiguriert")))?;
        let start = crate::connectors::gmail::begin_auth(&source.client_id, &source.redirect_uri);
        let url = start.url.clone();
        let mut pending = self.pending_auth.lock().await;
        pending.retain(|(i, _)| i != instance);
        pending.push((instance.to_string(), start));
        Ok(url)
    }

    /// Nimmt den Rueckkanal entgegen: `de.tory.app://oauth2?code=…&state=…`.
    pub async fn finish_gmail_auth(&self, redirect_url: &str) -> Result<String> {
        let parsed = url::Url::parse(redirect_url)
            .map_err(|e| Error::config(format!("Rueckleitung nicht lesbar: {e}")))?;
        let mut code = None;
        let mut state = None;
        for (key, value) in parsed.query_pairs() {
            match key.as_ref() {
                "code" => code = Some(value.to_string()),
                "state" => state = Some(value.to_string()),
                "error" => return Err(Error::config(format!("Google hat abgelehnt: {value}"))),
                _ => {}
            }
        }
        let code = code.ok_or_else(|| Error::config("Rueckleitung ohne Code"))?;
        let state = state.ok_or_else(|| Error::config("Rueckleitung ohne state"))?;

        let (instance, start) = {
            let mut pending = self.pending_auth.lock().await;
            let idx = pending
                .iter()
                .position(|(_, s)| s.state == state)
                // Ein unbekanntes `state` heisst: diese Rueckleitung gehoert
                // nicht zu einem Anmeldeversuch dieser App.
                .ok_or_else(|| Error::config("Unbekanntes state — Anmeldung erneut starten"))?;
            pending.remove(idx)
        };

        let config = self.config.lock().await.clone();
        let source = config
            .gmail
            .iter()
            .find(|s| s.common.instance == instance)
            .ok_or_else(|| Error::config("Konto wurde zwischenzeitlich entfernt"))?;

        let mut secrets = self.secrets.lock().await;
        crate::connectors::gmail::finish_auth(
            &self.http,
            &mut secrets,
            &instance,
            &source.client_id,
            &source.redirect_uri,
            &code,
            &start.code_verifier,
        )
        .await?;
        Ok(instance)
    }

    /// Das Tagesbriefing — der erste AI-Schritt der Roadmap.
    pub async fn brief(&self) -> Result<ai::Completion> {
        let config = self.config.lock().await.clone();
        let signals = {
            let store = self.store.lock().await;
            store.signals(Utc::now())?
        };
        let provider = {
            let secrets = self.secrets.lock().await;
            ai::provider(self.http.clone(), &config.ai, &secrets)?
        };
        let prompt = ai::brief_prompt(&signals, Utc::now(), config.ai.titles_only);
        provider.complete(&prompt, &config.ai).await
    }
}

fn load_config(path: &Path) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(Config::default());
    }
    Ok(serde_json::from_str(&raw)?)
}

fn urlencode(raw: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    utf8_percent_encode(raw, NON_ALPHANUMERIC).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Cadence, DashboardConfig, MindwtrSource, SourceCommon};

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tory-engine-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn mindwtr(instance: &str) -> MindwtrSource {
        MindwtrSource {
            common: SourceCommon::new(instance, "Mindwtr", Cadence::minutes(15)),
            base_url: "https://m.example.de".into(),
            token_key: format!("mindwtr.{instance}.token"),
            statuses: vec!["next".into()],
            include_undated: false,
            horizon_days: 7,
        }
    }

    #[tokio::test]
    async fn startet_ohne_konfiguration() {
        let dir = tempdir("fresh");
        let engine = Engine::open(&dir).unwrap();
        let dash = engine.dashboard().await.unwrap();
        assert!(dash.top.is_empty());
        assert!(dash.cards.is_empty());
        assert!(dash.alerts.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn konfiguration_ueberlebt_einen_neustart() {
        let dir = tempdir("persist");
        {
            let engine = Engine::open(&dir).unwrap();
            let config = Config { mindwtr: vec![mindwtr("haupt")], ..Config::default() };
            assert!(engine.save_config(config).await.unwrap().is_empty());
        }
        let engine = Engine::open(&dir).unwrap();
        assert_eq!(engine.config().await.mindwtr.len(), 1);
        assert_eq!(engine.dashboard().await.unwrap().cards.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn kaputte_konfiguration_wird_nicht_gespeichert() {
        let dir = tempdir("invalid");
        let engine = Engine::open(&dir).unwrap();
        let mut broken = mindwtr("haupt");
        broken.base_url = "noco.example.de".into(); // ohne Schema
        let problems = engine
            .save_config(Config { mindwtr: vec![broken], ..Config::default() })
            .await
            .unwrap();
        assert_eq!(problems.len(), 1);
        assert!(engine.config().await.mindwtr.is_empty(), "nichts uebernommen");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn entfernte_quelle_hinterlaesst_keine_karte() {
        let dir = tempdir("remove");
        let engine = Engine::open(&dir).unwrap();
        engine
            .save_config(Config { mindwtr: vec![mindwtr("haupt")], ..Config::default() })
            .await
            .unwrap();
        engine.set_secret("mindwtr.haupt.token", "geheim").await.unwrap();

        engine.save_config(Config::default()).await.unwrap();
        assert!(engine.dashboard().await.unwrap().cards.is_empty());
        assert!(
            engine.secret_names().await.is_empty(),
            "verwaistes Geheimnis wurde aufgeraeumt"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn kartenreihenfolge_folgt_der_konfiguration() {
        let dir = tempdir("order");
        let engine = Engine::open(&dir).unwrap();
        engine
            .save_config(Config {
                mindwtr: vec![mindwtr("a"), mindwtr("b")],
                dashboard: DashboardConfig {
                    card_order: vec!["mindwtr:b".into(), "mindwtr:a".into()],
                    ..DashboardConfig::default()
                },
                ..Config::default()
            })
            .await
            .unwrap();
        let ids: Vec<String> =
            engine.dashboard().await.unwrap().cards.iter().map(|c| c.source.id()).collect();
        assert_eq!(ids, vec!["mindwtr:b", "mindwtr:a"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn obsidian_aktion_wird_zum_deep_link() {
        let dir = tempdir("action");
        let engine = Engine::open(&dir).unwrap();
        let url = engine
            .act(&Action::OpenNote { vault: "Privat".into(), path: "Projekte/Haus Nord.md".into() })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(url, "obsidian://open?vault=Privat&file=Projekte%2FHaus%20Nord%2Emd");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn gmail_anmeldung_braucht_ein_konfiguriertes_konto() {
        let dir = tempdir("gmail");
        let engine = Engine::open(&dir).unwrap();
        assert!(engine.begin_gmail_auth("privat").await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn untergeschobene_rueckleitung_wird_abgewiesen() {
        let dir = tempdir("state");
        let engine = Engine::open(&dir).unwrap();
        let err = engine
            .finish_gmail_auth("de.tory.app://oauth2?code=abc&state=fremd")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("state"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn sync_ohne_quellen_meldet_nichts() {
        let dir = tempdir("sync");
        let engine = Engine::open(&dir).unwrap();
        let report = engine.sync(true).await.unwrap();
        assert!(report.synced.is_empty() && report.failed.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn brief_ohne_ai_schluessel_ist_ein_klarer_fehler() {
        let dir = tempdir("brief");
        let engine = Engine::open(&dir).unwrap();
        let err = engine.brief().await.unwrap_err();
        assert!(err.to_string().contains("ausgeschaltet"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn zeitzone_wird_begrenzt() {
        let dir = tempdir("tz");
        let engine = Engine::open(&dir).unwrap();
        engine.set_tz_offset(99999).await;
        assert_eq!(*engine.tz_offset.lock().await, 14 * 60);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
