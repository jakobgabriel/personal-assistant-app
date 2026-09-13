//! Mindwtr — selbst gehosteter GTD-Server.
//!
//! Angebunden wird der `mindwtr-cloud`-Dienst aus dem Docker-Stack, nicht die
//! lokale API der Desktop-App: die bindet auf `127.0.0.1` und ist vom Telefon
//! aus nicht erreichbar. Die Cloud liegt unter `/v1`, die Anmeldung ist ein
//! Bearer-Token aus `MINDWTR_CLOUD_AUTH_TOKENS`.
//!
//! Genutzte Endpunkte:
//!
//! | Zweck | Aufruf |
//! | --- | --- |
//! | Aufgaben lesen | `GET /v1/tasks?status=…&limit=…&offset=…` |
//! | Projekte lesen | `GET /v1/projects` |
//! | Abhaken | `POST /v1/tasks/{id}/complete` |
//!
//! Abhaken ist der einzige schreibende Aufruf, den Tory nach aussen macht.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

use crate::config::{MindwtrAccess, MindwtrSource};
use crate::error::{Error, Result};
use crate::http::{expect_ok, join};
use crate::model::{
    Action, Harvest, Overview, OverviewLine, Signal, SourceKind, SourceRef, TimeKind, Urgency,
};

use super::{shorten, urgency_for_at, urgency_for_due, Connector, SyncContext};

/// Eine Aufgabe, wie `/v1/tasks` sie liefert. Nur die Felder, die Tory braucht —
/// unbekannte Felder ignoriert serde, damit ein Mindwtr-Update den Sync nicht
/// bricht.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindwtrTask {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub is_focused_today: Option<bool>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub contexts: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MindwtrProject {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
struct TaskPage {
    #[serde(default)]
    tasks: Vec<MindwtrTask>,
    #[serde(default)]
    total: usize,
}

#[derive(Debug, Deserialize)]
struct ProjectPage {
    #[serde(default)]
    projects: Vec<MindwtrProject>,
}

/// Die `data.json` des WebDAV-Syncs. Dieselben Werte wie aus der REST-Antwort,
/// nur alle auf einmal und ohne Umschlag.
#[derive(Debug, Deserialize)]
struct AppData {
    #[serde(default)]
    tasks: Vec<MindwtrTask>,
    #[serde(default)]
    projects: Vec<MindwtrProject>,
}

pub struct MindwtrConnector {
    source: SourceRef,
    config: MindwtrSource,
}

impl MindwtrConnector {
    pub fn new(config: MindwtrSource) -> Self {
        let source = SourceRef::new(SourceKind::Mindwtr, &config.common.instance, &config.common.label);
        Self { source, config }
    }
}

#[async_trait]
impl Connector for MindwtrConnector {
    fn source(&self) -> &SourceRef {
        &self.source
    }

    async fn fetch(&self, ctx: &SyncContext<'_>) -> Result<Harvest> {
        match &self.config.access {
            MindwtrAccess::Cloud { base_url, token_key } => {
                self.fetch_cloud(ctx, base_url, token_key).await
            }
            MindwtrAccess::Webdav { url, username, password_key } => {
                self.fetch_webdav(ctx, url, username, password_key).await
            }
        }
    }

    async fn complete(&self, ctx: &SyncContext<'_>, item_id: &str) -> Result<()> {
        let MindwtrAccess::Cloud { base_url, token_key } = &self.config.access else {
            return Err(Error::config(
                "Ueber WebDAV liest Tory nur. Zum Abhaken die Cloud-Schnittstelle einrichten.",
            ));
        };
        let token = ctx.secrets.require(token_key)?;
        let url = join(&join(base_url, "v1"), &format!("tasks/{item_id}/complete"));
        expect_ok(ctx.http.post(url).bearer_auth(token).send().await?).await?;
        Ok(())
    }
}

impl MindwtrConnector {
    async fn fetch_cloud(
        &self,
        ctx: &SyncContext<'_>,
        base_url: &str,
        token_key: &str,
    ) -> Result<Harvest> {
        let token = ctx.secrets.require(token_key)?;
        let base = join(base_url, "v1");

        // Pro Status ein Aufruf: `status` nimmt nur einen Wert, und so bleibt
        // die Antwort klein.
        let mut tasks: Vec<MindwtrTask> = Vec::new();
        let mut total = 0usize;
        for status in &self.config.statuses {
            let page: TaskPage = expect_ok(
                ctx.http
                    .get(join(&base, "tasks"))
                    .bearer_auth(token)
                    .query(&[("status", status.as_str()), ("limit", "200")])
                    .send()
                    .await?,
            )
            .await?
            .json()
            .await?;
            total += page.total;
            tasks.extend(page.tasks);
        }

        let projects: Vec<MindwtrProject> = match expect_ok(
            ctx.http.get(join(&base, "projects")).bearer_auth(token).query(&[("limit", "200")]).send().await?,
        )
        .await
        {
            Ok(response) => response.json::<ProjectPage>().await?.projects,
            // Projektnamen sind Beschriftung, kein Inhalt — ohne sie geht es auch.
            Err(err) => {
                log::warn!("Mindwtr-Projekte nicht gelesen: {err}");
                Vec::new()
            }
        };

        Ok(map_tasks(&self.source, &self.config, tasks, &projects, ctx, total))
    }

    /// Ein `GET` auf die Datei, mehr ist es nicht. Die Zahl in der Karte kommt
    /// hier aus der Datei selbst statt aus einem `total` des Servers.
    async fn fetch_webdav(
        &self,
        ctx: &SyncContext<'_>,
        url: &str,
        username: &str,
        password_key: &str,
    ) -> Result<Harvest> {
        let password = ctx.secrets.require(password_key)?;
        let body = expect_ok(ctx.http.get(url).basic_auth(username, Some(password)).send().await?)
            .await?
            .text()
            .await?;
        let data = parse_app_data(&body)?;
        let total = data.tasks.len();
        Ok(map_tasks(&self.source, &self.config, data.tasks, &data.projects, ctx, total))
    }
}

/// Aufgaben -> Signale und Karte. Reine Funktion, gegen die Fixture getestet.
pub fn map_tasks(
    source: &SourceRef,
    config: &MindwtrSource,
    tasks: Vec<MindwtrTask>,
    projects: &[MindwtrProject],
    ctx: &SyncContext<'_>,
    total_reported: usize,
) -> Harvest {
    let names: HashMap<&str, &str> =
        projects.iter().map(|p| (p.id.as_str(), p.title.as_str())).collect();
    let today = ctx.today();
    let horizon = today + chrono::Duration::days(config.horizon_days.max(0));
    let abhakbar = config.access.can_complete();

    let open: Vec<&MindwtrTask> = tasks
        .iter()
        .filter(|t| t.deleted_at.is_none() && t.completed_at.is_none())
        .filter(|t| !matches!(t.status.as_str(), "done" | "archived"))
        .collect();

    let mut overdue = 0usize;
    let mut today_count = 0usize;
    let mut focus = 0usize;
    let mut signals = Vec::new();

    for task in &open {
        let due = task.due_date.as_deref().and_then(parse_date);
        let start = task.start_time.as_deref().and_then(parse_instant);
        let focused = task.is_focused_today.unwrap_or(false);
        if focused {
            focus += 1;
        }
        if let Some(d) = due {
            if d < today {
                overdue += 1;
            } else if d == today {
                today_count += 1;
            }
        }

        // Was nicht in den Horizont faellt, gehoert nicht auf den Startscreen —
        // es sei denn, es ist als Tagesfokus markiert oder der Nutzer will
        // ausdruecklich alles sehen.
        let relevant = focused
            || config.include_undated
            || due.is_some_and(|d| d <= horizon)
            || start.is_some_and(|s| s.date_naive() <= horizon);
        if !relevant {
            continue;
        }

        let (at, time_kind, mut urgency) = match (due, start) {
            (Some(d), _) => (
                Some(ctx.local_date_start(d)),
                Some(TimeKind::Due),
                urgency_for_due(d, today),
            ),
            (None, Some(s)) => (Some(s), Some(TimeKind::At), urgency_for_at(s, ctx.now)),
            (None, None) => (None, None, Urgency::Normal),
        };
        if focused && urgency > Urgency::High {
            // Tagesfokus heisst: heute. Das hebt ein datumsloses Signal an.
            urgency = Urgency::High;
        }
        if matches!(task.priority.as_deref(), Some("urgent")) && urgency > Urgency::High {
            urgency = Urgency::High;
        }

        let project = task.project_id.as_deref().and_then(|id| names.get(id)).copied();
        let mut parts: Vec<String> = Vec::new();
        if let Some(p) = project {
            parts.push(p.to_string());
        }
        parts.push(status_label(&task.status).to_string());
        parts.extend(task.contexts.iter().cloned());

        signals.push(Signal {
            subtitle: Some(parts.join(" · ")),
            excerpt: task.description.as_deref().map(|d| shorten(d, 140)).filter(|d| !d.is_empty()),
            at,
            time_kind,
            urgency,
            badge: project.map(|p| p.to_string()).or_else(|| Some(status_label(&task.status).into())),
            tags: task.tags.clone(),
            // Ueber WebDAV waere der Haken eine Luege: er kaeme nirgends an.
            action: abhakbar
                .then(|| Action::CompleteTask { source: source.id(), task_id: task.id.clone() }),
            completable: abhakbar,
            dedup_key: Some(format!("task:{}", task.title.to_lowercase())),
            ..Signal::new(source.clone(), task.id.clone(), task.title.clone())
        });
    }

    let open_count = open.len();
    let mut lines = Vec::new();
    if overdue > 0 {
        lines.push(OverviewLine {
            text: format!("{overdue} ueberfaellig"),
            note: None,
            urgency: Some(Urgency::Critical),
        });
    }
    if today_count > 0 {
        lines.push(OverviewLine {
            text: format!("{today_count} heute faellig"),
            note: None,
            urgency: Some(Urgency::High),
        });
    }
    if focus > 0 {
        lines.push(OverviewLine { text: format!("{focus} im Tagesfokus"), note: None, urgency: None });
    }
    let inbox = open.iter().filter(|t| t.status == "inbox").count();
    if inbox > 0 {
        lines.push(OverviewLine {
            text: format!("{inbox} im Eingang"),
            note: Some("noch nicht geklaert".into()),
            urgency: None,
        });
    }

    Harvest {
        overview: Overview {
            source: source.clone(),
            metric: open_count.to_string(),
            metric_raw: Some(open_count as f64),
            caption: "Aufgaben offen".into(),
            note: (overdue > 0).then(|| format!("{overdue} ueberfaellig")),
            lines,
            // Der Server meldet `total` inklusive Erledigtem — daraus laesst
            // sich der Fortschritt ableiten, sobald er groesser als offen ist.
            progress: (total_reported > open_count)
                .then(|| 1.0 - (open_count as f32 / total_reported as f32)),
        },
        source: source.clone(),
        signals,
    }
}

/// Liest die `data.json`. Ein nicht-JSON-Koerper heisst hier fast immer eines
/// von zwei Dingen, und beide soll der Nutzer lesen koennen statt "expected
/// value at line 1".
fn parse_app_data(body: &str) -> Result<AppData> {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('{') {
        let anfang: String = trimmed.chars().take(80).collect();
        return Err(Error::Sync(crate::model::SyncFault::Misconfigured {
            detail: if trimmed.starts_with('<') {
                format!(
                    "Die URL liefert HTML statt JSON — sie zeigt vermutlich auf einen Ordner \
                     oder eine Anmeldeseite statt auf die data.json. Anfang: {anfang}"
                )
            } else {
                format!(
                    "Die Datei ist kein JSON. Bei eingeschalteter Sync-Verschluesselung heisst \
                     sie data.enc.json und ist fuer Tory nicht lesbar. Anfang: {anfang}"
                )
            },
        }));
    }
    serde_json::from_str(trimmed).map_err(Error::from)
}

/// Der Schluesselname, unter dem das Token liegt.
pub fn token_key(instance: &str) -> String {
    format!("mindwtr.{instance}.token")
}

/// Der Schluesselname, unter dem das WebDAV-Passwort liegt.
pub fn password_key(instance: &str) -> String {
    format!("mindwtr.{instance}.webdav")
}

fn status_label(status: &str) -> &str {
    match status {
        "inbox" => "Eingang",
        "next" => "Naechste",
        "waiting" => "Wartet",
        "someday" => "Irgendwann",
        "reference" => "Referenz",
        "done" => "Erledigt",
        "archived" => "Archiv",
        other => other,
    }
}

/// Mindwtr schreibt Datumsfelder entweder als `2026-09-12` oder als voller
/// Zeitstempel. Beides muss zum Datum fuehren.
fn parse_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .or_else(|| DateTime::parse_from_rfc3339(raw).ok().map(|t| t.naive_utc().date()))
}

fn parse_instant(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw).ok().map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Cadence, SourceCommon};
    use crate::secrets::SecretStore;
    use chrono::TimeZone;

    fn quelle() -> SourceRef {
        SourceRef::new(SourceKind::Mindwtr, "haupt", "Mindwtr")
    }

    fn konfig() -> MindwtrSource {
        MindwtrSource {
            common: SourceCommon::new("haupt", "Mindwtr", Cadence::minutes(15)),
            access: MindwtrAccess::Cloud {
                base_url: "https://mindwtr.example.de".into(),
                token_key: token_key("haupt"),
            },
            statuses: vec!["inbox".into(), "next".into(), "waiting".into()],
            include_undated: false,
            horizon_days: 7,
        }
    }

    fn konfig_webdav() -> MindwtrSource {
        MindwtrSource {
            access: MindwtrAccess::Webdav {
                url: "https://cloud.example.de/dav/Mindwtr/data.json".into(),
                username: "jakob".into(),
                password_key: password_key("haupt"),
            },
            ..konfig()
        }
    }

    struct Umgebung {
        dir: std::path::PathBuf,
        secrets: SecretStore,
        http: reqwest::Client,
    }

    impl Umgebung {
        fn neu(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("tory-mindwtr-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self {
                secrets: SecretStore::open(&dir).unwrap(),
                http: crate::http::client().unwrap(),
                dir,
            }
        }
        fn ctx(&self) -> SyncContext<'_> {
            SyncContext {
                http: &self.http,
                secrets: &self.secrets,
                // Fest, damit die Fixture-Daten reproduzierbar bewertet werden.
                now: Utc.with_ymd_and_hms(2026, 9, 11, 9, 0, 0).unwrap(),
                tz_offset_minutes: 120,
            }
        }
    }

    impl Drop for Umgebung {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn fixture() -> (Vec<MindwtrTask>, Vec<MindwtrProject>) {
        let raw = include_str!("../../tests/fixtures/mindwtr_tasks.json");
        let page: TaskPage = serde_json::from_str(raw).unwrap();
        let projects: ProjectPage =
            serde_json::from_str(include_str!("../../tests/fixtures/mindwtr_projects.json")).unwrap();
        (page.tasks, projects.projects)
    }

    #[test]
    fn liest_die_echte_antwortform() {
        let (tasks, projects) = fixture();
        assert_eq!(tasks.len(), 6);
        assert_eq!(projects.len(), 2);
        assert_eq!(tasks[0].title, "Steuerunterlagen zusammenstellen");
        assert_eq!(tasks[0].status, "next");
    }

    #[test]
    fn ueberfaelliges_ist_kritisch_morgen_ist_hoch() {
        let u = Umgebung::neu("urgency");
        let (tasks, projects) = fixture();
        let h = map_tasks(&quelle(), &konfig(), tasks, &projects, &u.ctx(), 20);

        let ueberfaellig = h.signals.iter().find(|s| s.title.starts_with("Steuerunterlagen")).unwrap();
        assert_eq!(ueberfaellig.urgency, Urgency::Critical, "9.9. liegt vor dem 11.9.");

        let morgen = h.signals.iter().find(|s| s.title.starts_with("Angebot")).unwrap();
        assert_eq!(morgen.urgency, Urgency::High);
    }

    #[test]
    fn projektname_wird_zur_beschriftung() {
        let u = Umgebung::neu("project");
        let (tasks, projects) = fixture();
        let h = map_tasks(&quelle(), &konfig(), tasks, &projects, &u.ctx(), 20);
        let s = h.signals.iter().find(|s| s.title.starts_with("Steuerunterlagen")).unwrap();
        assert_eq!(s.badge.as_deref(), Some("Finanzen 2026"));
        assert!(s.subtitle.as_deref().unwrap().starts_with("Finanzen 2026 · Naechste"));
    }

    #[test]
    fn erledigtes_und_geloeschtes_kommt_nicht_vor() {
        let u = Umgebung::neu("done");
        let (tasks, projects) = fixture();
        let h = map_tasks(&quelle(), &konfig(), tasks, &projects, &u.ctx(), 20);
        assert!(!h.signals.iter().any(|s| s.title.contains("Schon erledigt")));
        assert!(!h.signals.iter().any(|s| s.title.contains("Geloescht")));
        assert_eq!(h.overview.metric, "4", "vier offene Aufgaben in der Fixture");
    }

    #[test]
    fn datumslose_aufgabe_bleibt_aus_wenn_nicht_gewuenscht() {
        let u = Umgebung::neu("undated");
        let (tasks, projects) = fixture();
        let ohne = map_tasks(&quelle(), &konfig(), tasks.clone(), &projects, &u.ctx(), 20);
        assert!(!ohne.signals.iter().any(|s| s.title.starts_with("Irgendwann mal")));

        let mut config = konfig();
        config.include_undated = true;
        let mit = map_tasks(&quelle(), &config, tasks, &projects, &u.ctx(), 20);
        assert!(mit.signals.iter().any(|s| s.title.starts_with("Irgendwann mal")));
    }

    #[test]
    fn tagesfokus_kommt_auch_ohne_datum_durch() {
        let u = Umgebung::neu("focus");
        let (tasks, projects) = fixture();
        let h = map_tasks(&quelle(), &konfig(), tasks, &projects, &u.ctx(), 20);
        let fokus = h.signals.iter().find(|s| s.title.starts_with("Heute anrufen")).unwrap();
        assert_eq!(fokus.urgency, Urgency::High);
        assert!(fokus.at.is_none(), "kein Datum, aber trotzdem sichtbar");
    }

    #[test]
    fn karte_zaehlt_ueberfaellig_und_eingang() {
        let u = Umgebung::neu("card");
        let (tasks, projects) = fixture();
        let h = map_tasks(&quelle(), &konfig(), tasks, &projects, &u.ctx(), 20);
        assert_eq!(h.overview.note.as_deref(), Some("1 ueberfaellig"));
        assert!(h.overview.lines.iter().any(|l| l.text == "1 ueberfaellig"));
        assert!(h.overview.lines.iter().any(|l| l.text.contains("im Eingang")));
        assert!(h.overview.progress.unwrap() > 0.0);
    }

    #[test]
    fn signale_sind_abhakbar_und_zeigen_auf_die_richtige_aufgabe() {
        let u = Umgebung::neu("action");
        let (tasks, projects) = fixture();
        let h = map_tasks(&quelle(), &konfig(), tasks, &projects, &u.ctx(), 20);
        let s = &h.signals[0];
        assert!(s.completable);
        match s.action.as_ref().unwrap() {
            Action::CompleteTask { source, task_id } => {
                assert_eq!(source, "mindwtr:haupt");
                assert_eq!(task_id, &s.id);
            }
            other => panic!("falsche Aktion: {other:?}"),
        }
    }

    #[test]
    fn ueber_webdav_ist_nichts_abhakbar() {
        let u = Umgebung::neu("webdav-readonly");
        let (tasks, projects) = fixture();
        let h = map_tasks(&quelle(), &konfig_webdav(), tasks, &projects, &u.ctx(), 20);
        assert!(!h.signals.is_empty());
        assert!(
            h.signals.iter().all(|s| !s.completable && s.action.is_none()),
            "ein Haken, der nirgends ankommt, gehoert nicht in die Zeile"
        );
    }

    #[test]
    fn die_webdav_datei_liefert_dieselben_aufgaben_wie_die_rest_antwort() {
        // Beide Wege tragen dieselben `Task`-Werte; nur die Huelle unterscheidet
        // sich. Genau darum bleibt die Abbildung darunter unveraendert.
        let data = parse_app_data(include_str!("../../tests/fixtures/mindwtr_data.json")).unwrap();
        assert_eq!(data.tasks.len(), 6);
        assert_eq!(data.projects.len(), 2);
        assert_eq!(data.tasks[0].title, "Steuerunterlagen zusammenstellen");

        let u = Umgebung::neu("webdav-parity");
        let ueber_datei =
            map_tasks(&quelle(), &konfig(), data.tasks, &data.projects, &u.ctx(), 20);
        let (tasks, projects) = fixture();
        let ueber_rest = map_tasks(&quelle(), &konfig(), tasks, &projects, &u.ctx(), 20);

        let titel = |h: &Harvest| h.signals.iter().map(|s| s.title.clone()).collect::<Vec<_>>();
        assert_eq!(titel(&ueber_datei), titel(&ueber_rest));
        assert_eq!(ueber_datei.overview.metric, ueber_rest.overview.metric);
    }

    #[test]
    fn html_statt_json_wird_erklaert() {
        let err = parse_app_data("<!doctype html><html><body>Login</body></html>").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("HTML"), "{text}");
        assert!(text.contains("Ordner"), "sagt, was wahrscheinlich falsch ist: {text}");
    }

    #[test]
    fn verschluesselte_datei_wird_erklaert() {
        let err = parse_app_data("\u{1}\u{2}binaerer Kram").unwrap_err();
        assert!(err.to_string().contains("data.enc.json"), "{err}");
    }

    #[test]
    fn kaputte_konfiguration_faellt_vor_dem_sync_auf() {
        use crate::config::Config;
        let mut verschluesselt = konfig_webdav();
        verschluesselt.access = MindwtrAccess::Webdav {
            url: "https://cloud.example.de/dav/Mindwtr/data.enc.json".into(),
            username: "jakob".into(),
            password_key: password_key("haupt"),
        };
        let probleme = Config { mindwtr: vec![verschluesselt], ..Config::default() }.validate();
        assert_eq!(probleme.len(), 1);
        assert!(probleme[0].contains("verschluesselte"), "{:?}", probleme);

        let mut ordner = konfig_webdav();
        ordner.access = MindwtrAccess::Webdav {
            url: "https://cloud.example.de/dav/Mindwtr/".into(),
            username: "jakob".into(),
            password_key: password_key("haupt"),
        };
        let probleme = Config { mindwtr: vec![ordner], ..Config::default() }.validate();
        assert!(probleme.iter().any(|p| p.contains("data.json")), "{:?}", probleme);
    }

    #[test]
    fn datum_ohne_uhrzeit_und_mit_zeitstempel() {
        assert_eq!(parse_date("2026-09-12"), NaiveDate::from_ymd_opt(2026, 9, 12));
        assert_eq!(parse_date("2026-09-12T10:00:00Z"), NaiveDate::from_ymd_opt(2026, 9, 12));
        assert_eq!(parse_date("unsinn"), None);
    }
}
