//! Obsidian — beliebig viele Vaults.
//!
//! Ein Vault ist ein Ordner mit Markdown; Obsidian selbst hat keine
//! Schnittstelle. Deshalb greift Tory auf die Dateien zu, auf einem von zwei
//! Wegen ([`VaultAccess`]):
//!
//! * **Local** — der Ordner selbst. Auf dem Desktop der Normalfall; auf dem
//!   Telefon nur, wenn ein Ordner lokal gespiegelt wird (Syncthing, FolderSync).
//! * **Webdav** — Nextcloud, `rclone serve webdav`, jeder WebDAV-Server. Das ist
//!   der Weg, der vom Telefon aus zuverlaessig funktioniert.
//!
//! Beide liefern dieselbe Liste von Dateien; ab dort ist der Code identisch.
//! Gelesen werden offene Checkboxen (als Aufgabensignal) und Notizen mit einem
//! der `pinned_tags` (als Merker). Der Volltext bleibt im Vault — angetippt
//! springt Tory per `obsidian://open` in die App, die das besser kann.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::config::{ObsidianSource, VaultAccess};
use crate::error::{Error, Result};
use crate::markdown::{parse_note, Note};
use crate::model::{
    Action, Harvest, Overview, OverviewLine, Signal, SourceKind, SourceRef, TimeKind, Urgency,
};

use super::{shorten, urgency_for_due, Connector, SyncContext};

/// Eine Markdown-Datei im Vault, mit vaultrelativem Pfad.
#[derive(Debug, Clone, PartialEq)]
pub struct VaultFile {
    /// Pfad relativ zur Vault-Wurzel, mit `/` als Trenner.
    pub path: String,
    pub content: String,
    pub modified: Option<DateTime<Utc>>,
}

pub struct ObsidianConnector {
    source: SourceRef,
    config: ObsidianSource,
}

impl ObsidianConnector {
    pub fn new(config: ObsidianSource) -> Self {
        let source = SourceRef::new(SourceKind::Obsidian, &config.common.instance, &config.common.label);
        Self { source, config }
    }
}

#[async_trait]
impl Connector for ObsidianConnector {
    fn source(&self) -> &SourceRef {
        &self.source
    }

    async fn fetch(&self, ctx: &SyncContext<'_>) -> Result<Harvest> {
        let files = match &self.config.access {
            VaultAccess::Local { path } => read_local(Path::new(path), &self.config)?,
            VaultAccess::Webdav { base_url, username, password_key } => {
                let password = ctx.secrets.require(password_key)?;
                read_webdav(ctx, base_url, username, password, &self.config).await?
            }
        };
        Ok(map_files(&self.source, &self.config, &files, ctx))
    }
}

/// Liest einen Vault aus dem Dateisystem.
pub fn read_local(root: &Path, config: &ObsidianSource) -> Result<Vec<VaultFile>> {
    if !root.is_dir() {
        return Err(Error::config(format!("Vault-Ordner nicht gefunden: {}", root.display())));
    }
    let mut files = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if files.len() >= config.scan_limit {
            break;
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let rel = relative(root, &path);
            if path.is_dir() {
                if !skip_dir(&name, &rel, config) {
                    stack.push(path);
                }
                continue;
            }
            if !name.ends_with(".md") || files.len() >= config.scan_limit {
                continue;
            }
            if !included(&rel, config) {
                continue;
            }
            files.push(VaultFile {
                path: rel,
                content: std::fs::read_to_string(&path).unwrap_or_default(),
                modified: entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(DateTime::<Utc>::from),
            });
        }
    }
    Ok(files)
}

/// Liest einen Vault ueber WebDAV: ein `PROPFIND` fuer die Dateiliste, dann ein
/// `GET` pro Markdown-Datei.
///
/// `Depth: infinity` waere ein Aufruf statt vieler, aber viele Server
/// (Nextcloud) verbieten es. Deshalb `Depth: 1` pro Ordner.
pub async fn read_webdav(
    ctx: &SyncContext<'_>,
    base_url: &str,
    username: &str,
    password: &str,
    config: &ObsidianSource,
) -> Result<Vec<VaultFile>> {
    let base = base_url.trim_end_matches('/').to_string();
    let mut files = Vec::new();
    let mut dirs: Vec<String> = vec![String::new()];
    let mut visited = HashSet::new();

    while let Some(dir) = dirs.pop() {
        if files.len() >= config.scan_limit {
            break;
        }
        if !visited.insert(dir.clone()) {
            continue;
        }
        let url = if dir.is_empty() { base.clone() } else { format!("{base}/{}", encode_path(&dir)) };
        let response = crate::http::expect_ok(
            ctx.http
                .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
                .basic_auth(username, Some(password))
                .header("Depth", "1")
                .header(reqwest::header::CONTENT_TYPE, "application/xml")
                .body(PROPFIND_BODY)
                .send()
                .await?,
        )
        .await?;
        let xml = response.text().await?;

        for entry in parse_propfind(&xml, &base) {
            let rel = if dir.is_empty() { entry.name.clone() } else { format!("{dir}/{}", entry.name) };
            if rel.is_empty() || rel == dir {
                continue;
            }
            if entry.is_dir {
                if !skip_dir(&entry.name, &rel, config) {
                    dirs.push(rel);
                }
                continue;
            }
            if !rel.ends_with(".md") || !included(&rel, config) || files.len() >= config.scan_limit {
                continue;
            }
            let file_url = format!("{base}/{}", encode_path(&rel));
            let content = crate::http::expect_ok(
                ctx.http.get(&file_url).basic_auth(username, Some(password)).send().await?,
            )
            .await?
            .text()
            .await?;
            files.push(VaultFile { path: rel, content, modified: entry.modified });
        }
    }
    Ok(files)
}

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop>
<d:resourcetype/><d:getlastmodified/><d:getcontentlength/>
</d:prop></d:propfind>"#;

/// Ein Eintrag aus einer PROPFIND-Antwort.
#[derive(Debug, Clone, PartialEq)]
pub struct DavEntry {
    /// Nur der letzte Pfadteil, dekodiert.
    pub name: String,
    pub is_dir: bool,
    pub modified: Option<DateTime<Utc>>,
}

/// Liest eine PROPFIND-Antwort.
///
/// Ereignisbasiert und namensraum-unabhaengig: WebDAV-Server unterscheiden sich
/// in den Praefixen (`d:`, `D:`, keins) und darin, ob `href` absolut oder
/// pfadrelativ ist. Deshalb wird nur der lokale Elementname betrachtet.
pub fn parse_propfind(xml: &str, base_url: &str) -> Vec<DavEntry> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let base_path = url::Url::parse(base_url)
        .map(|u| decode_path(u.path().trim_end_matches('/')))
        .unwrap_or_default();

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut out = Vec::new();
    let mut href: Option<String> = None;
    let mut is_dir = false;
    let mut modified: Option<DateTime<Utc>> = None;
    let mut field: Option<Field> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(tag)) | Ok(Event::Empty(tag)) => match tag.local_name().as_ref() {
                b"response" => {
                    href = None;
                    is_dir = false;
                    modified = None;
                    field = None;
                }
                b"collection" => is_dir = true,
                b"href" => field = Some(Field::Href),
                b"getlastmodified" => field = Some(Field::Modified),
                _ => field = None,
            },
            Ok(Event::Text(text)) => {
                let Ok(value) = text.unescape() else { continue };
                let value = value.trim().to_string();
                match field {
                    Some(Field::Href) => href = Some(value),
                    Some(Field::Modified) => modified = parse_http_date(&value),
                    None => {}
                }
            }
            Ok(Event::End(tag)) => {
                if tag.local_name().as_ref() == b"response" {
                    if let Some(name) = entry_name(href.as_deref(), &base_path) {
                        out.push(DavEntry { name, is_dir, modified });
                    }
                }
                field = None;
            }
            Ok(Event::Eof) => break,
            // Eine kaputte Antwort liefert lieber die bis dahin gelesenen
            // Eintraege als gar nichts: der Vault ist dann unvollstaendig,
            // aber nicht unsichtbar.
            Err(err) => {
                log::warn!("PROPFIND nicht vollstaendig gelesen: {err}");
                break;
            }
            _ => {}
        }
    }
    out
}

/// Welches Textfeld gerade gelesen wird.
#[derive(Clone, Copy)]
enum Field {
    Href,
    Modified,
}

/// Liest `getlastmodified`, das WebDAV als HTTP-Datum schreibt
/// (`Fri, 11 Sep 2026 06:00:00 GMT`).
///
/// Zuerst streng ueber RFC 2822. Der Fallback ignoriert den Wochentag: manche
/// Server rechnen ihn falsch aus, und ein falscher Wochentag ist kein Grund,
/// den Zeitstempel wegzuwerfen.
fn parse_http_date(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(t) = DateTime::parse_from_rfc2822(value) {
        return Some(t.with_timezone(&Utc));
    }
    let ohne_wochentag = value.split_once(", ").map(|(_, rest)| rest).unwrap_or(value);
    chrono::NaiveDateTime::parse_from_str(ohne_wochentag.trim(), "%d %b %Y %H:%M:%S GMT")
        .ok()
        .map(|naive| chrono::TimeZone::from_utc_datetime(&Utc, &naive))
}

/// Letzter Pfadteil eines `href`, relativ zur angefragten Basis. `None` fuer den
/// angefragten Ordner selbst — der steht in jeder PROPFIND-Antwort mit drin.
fn entry_name(href: Option<&str>, base_path: &str) -> Option<String> {
    let href = href?.trim();
    if href.is_empty() {
        return None;
    }
    // Absolut mit Host, oder pfadrelativ.
    let path = match href.split_once("://") {
        Some((_, rest)) => rest.split_once('/').map(|(_, p)| format!("/{p}"))?,
        None => href.to_string(),
    };
    let decoded = decode_path(path.trim_end_matches('/'));
    let rel = decoded.strip_prefix(base_path).unwrap_or(&decoded).trim_start_matches('/');
    if rel.is_empty() {
        return None;
    }
    Some(rel.rsplit('/').next().unwrap_or(rel).to_string())
}

/// Notizen -> Signale und Karte. Reine Funktion, ohne Netz und Dateisystem.
pub fn map_files(
    source: &SourceRef,
    config: &ObsidianSource,
    files: &[VaultFile],
    ctx: &SyncContext<'_>,
) -> Harvest {
    let today = ctx.today();
    let mut signals = Vec::new();
    let mut open_tasks = 0usize;
    let mut overdue = 0usize;
    let mut dated = 0usize;
    let mut pinned = 0usize;

    for file in files {
        let note: Note = parse_note(&file.content);
        let note_title = note.title.clone().unwrap_or_else(|| stem(&file.path));

        if config.read_tasks {
            for task in &note.open_tasks {
                open_tasks += 1;
                let due = task.due.or(task.scheduled);
                if let Some(d) = due {
                    dated += 1;
                    if d < today {
                        overdue += 1;
                    }
                }
                // Ohne Datum wuerde ein grosser Vault den Startscreen fluten.
                // Datumslose Aufgaben zaehlen in der Karte, aber nur markierte
                // (hoch priorisiert) werden Signal.
                let Some(urgency) = due
                    .map(|d| urgency_for_due(d, today))
                    .or_else(|| task.high_priority.then_some(Urgency::Normal))
                else {
                    continue;
                };

                signals.push(Signal {
                    subtitle: Some(format!("{} · {note_title}", config.common.label)),
                    at: due.map(|d| ctx.local_date_start(d)),
                    time_kind: due.map(|_| TimeKind::Due),
                    urgency,
                    badge: Some(folder_of(&file.path).unwrap_or_else(|| config.vault_name.clone())),
                    tags: task.tags.clone(),
                    action: Some(Action::OpenNote {
                        vault: config.vault_name.clone(),
                        path: file.path.clone(),
                    }),
                    dedup_key: Some(format!("task:{}", task.text.to_lowercase())),
                    ..Signal::new(
                        source.clone(),
                        format!("task:{}:{}", file.path, task.line),
                        shorten(&task.text, 90),
                    )
                });
            }
        }

        let hits: Vec<&String> = config
            .pinned_tags
            .iter()
            .filter(|wanted| note.tags.iter().any(|t| t.eq_ignore_ascii_case(wanted)))
            .collect();
        if !hits.is_empty() {
            pinned += 1;
            signals.push(Signal {
                subtitle: Some(format!(
                    "{} · #{}",
                    config.common.label,
                    hits.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(" #")
                )),
                excerpt: note.excerpt.clone(),
                at: file.modified,
                time_kind: file.modified.map(|_| TimeKind::Since),
                urgency: Urgency::Normal,
                badge: Some(folder_of(&file.path).unwrap_or_else(|| config.vault_name.clone())),
                tags: note.tags.clone(),
                action: Some(Action::OpenNote {
                    vault: config.vault_name.clone(),
                    path: file.path.clone(),
                }),
                ..Signal::new(source.clone(), format!("note:{}", file.path), note_title.clone())
            });
        }
    }

    let mut lines = vec![OverviewLine {
        text: format!("{} Notizen gelesen", files.len()),
        note: None,
        urgency: None,
    }];
    if overdue > 0 {
        lines.insert(
            0,
            OverviewLine {
                text: format!("{overdue} ueberfaellig"),
                note: None,
                urgency: Some(Urgency::Critical),
            },
        );
    }
    if open_tasks > dated {
        lines.push(OverviewLine {
            text: format!("{} ohne Datum", open_tasks - dated),
            note: Some("nur in Obsidian sichtbar".into()),
            urgency: None,
        });
    }
    if pinned > 0 {
        lines.push(OverviewLine { text: format!("{pinned} angepinnt"), note: None, urgency: None });
    }

    Harvest {
        overview: Overview {
            source: source.clone(),
            metric: open_tasks.to_string(),
            metric_raw: Some(open_tasks as f64),
            caption: "offene Aufgaben".into(),
            note: (overdue > 0).then(|| format!("{overdue} ueberfaellig")),
            lines,
            progress: None,
        },
        source: source.clone(),
        signals,
    }
}

pub fn password_key(instance: &str) -> String {
    format!("obsidian.{instance}.webdav")
}

/// Ordner, die gar nicht betreten werden. `.obsidian` enthaelt Einstellungen,
/// `.trash` Geloeschtes — beides nie ein Signal.
fn skip_dir(name: &str, rel: &str, config: &ObsidianSource) -> bool {
    if name.starts_with('.') {
        return true;
    }
    if config.exclude_folders.iter().any(|e| e == name || rel.starts_with(e.as_str())) {
        return true;
    }
    // Bei gesetzten include_folders nur Aeste betreten, die dorthin fuehren.
    if !config.include_folders.is_empty() {
        return !config
            .include_folders
            .iter()
            .any(|inc| inc.starts_with(rel) || rel.starts_with(inc.as_str()));
    }
    false
}

fn included(rel: &str, config: &ObsidianSource) -> bool {
    if config.exclude_folders.iter().any(|e| rel.starts_with(e.as_str())) {
        return false;
    }
    config.include_folders.is_empty()
        || config.include_folders.iter().any(|inc| rel.starts_with(inc.as_str()))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Dateiname ohne `.md` — der Notiztitel, wenn die Notiz keinen nennt.
fn stem(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md").to_string()
}

/// Oberster Ordner des Pfades; im Vault ist das meist das Thema.
fn folder_of(path: &str) -> Option<String> {
    path.split_once('/').map(|(head, _)| head.to_string())
}

/// Zeichen, die in einem Pfadsegment kodiert werden muessen. Die nach RFC 3986
/// unreservierten `-._~` bleiben stehen — sonst wird aus `Notiz.md` ein
/// `Notiz%2Emd`, und manche WebDAV-Server finden das nicht.
const PATH_SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|seg| percent_encoding::utf8_percent_encode(seg, PATH_SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn decode_path(path: &str) -> String {
    percent_encoding::percent_decode_str(path).decode_utf8_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Cadence, SourceCommon};
    use crate::secrets::SecretStore;
    use chrono::TimeZone;

    fn quelle() -> SourceRef {
        SourceRef::new(SourceKind::Obsidian, "privat", "Vault Privat")
    }

    fn konfig() -> ObsidianSource {
        ObsidianSource {
            common: SourceCommon::new("privat", "Vault Privat", Cadence::minutes(30)),
            vault_name: "Privat".into(),
            access: VaultAccess::Local { path: "/nicht/benutzt".into() },
            include_folders: Vec::new(),
            exclude_folders: vec![".obsidian".into(), "Archiv".into()],
            read_tasks: true,
            pinned_tags: vec!["merker".into()],
            scan_limit: 400,
        }
    }

    struct Umgebung {
        dir: std::path::PathBuf,
        secrets: SecretStore,
        http: reqwest::Client,
    }

    impl Umgebung {
        fn neu(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("tory-obs-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self { secrets: SecretStore::open(&dir).unwrap(), http: crate::http::client().unwrap(), dir }
        }
        fn ctx(&self) -> SyncContext<'_> {
            SyncContext {
                http: &self.http,
                secrets: &self.secrets,
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

    fn datei(path: &str, content: &str) -> VaultFile {
        VaultFile { path: path.into(), content: content.into(), modified: None }
    }

    #[test]
    fn aufgaben_mit_frist_werden_signale() {
        let u = Umgebung::neu("tasks");
        let files = vec![datei(
            "Projekte/Haus.md",
            "# Haus\n- [ ] Dach pruefen \u{1F4C5} 2026-09-09\n- [ ] Fenster putzen \u{1F4C5} 2026-09-12\n",
        )];
        let h = map_files(&quelle(), &konfig(), &files, &u.ctx());
        assert_eq!(h.signals.len(), 2);
        let dach = h.signals.iter().find(|s| s.title.contains("Dach")).unwrap();
        assert_eq!(dach.urgency, Urgency::Critical);
        assert_eq!(dach.badge.as_deref(), Some("Projekte"));
        assert_eq!(dach.subtitle.as_deref(), Some("Vault Privat · Haus"));
    }

    #[test]
    fn datumslose_aufgaben_zaehlen_aber_fluten_nicht() {
        let u = Umgebung::neu("undated");
        let files = vec![datei("Notizen/Kram.md", "- [ ] Irgendwas\n- [ ] Noch was\n")];
        let h = map_files(&quelle(), &konfig(), &files, &u.ctx());
        assert!(h.signals.is_empty(), "ohne Datum kein Signal");
        assert_eq!(h.overview.metric, "2", "gezaehlt werden sie trotzdem");
        assert!(h.overview.lines.iter().any(|l| l.text == "2 ohne Datum"));
    }

    #[test]
    fn hoch_priorisierte_aufgabe_kommt_auch_ohne_datum_durch() {
        let u = Umgebung::neu("prio");
        let files = vec![datei("Notizen/Kram.md", "- [ ] Wichtig \u{23EB}\n- [ ] Unwichtig\n")];
        let h = map_files(&quelle(), &konfig(), &files, &u.ctx());
        assert_eq!(h.signals.len(), 1);
        assert!(h.signals[0].title.contains("Wichtig"));
    }

    #[test]
    fn angepinnte_notiz_wird_signal() {
        let u = Umgebung::neu("pinned");
        let files = vec![datei(
            "Merker/Umzug.md",
            "---\ntitle: Umzug Checkliste\ntags: [merker, wohnen]\n---\nKartons bestellen.\n",
        )];
        let h = map_files(&quelle(), &konfig(), &files, &u.ctx());
        assert_eq!(h.signals.len(), 1);
        let s = &h.signals[0];
        assert_eq!(s.title, "Umzug Checkliste");
        assert_eq!(s.excerpt.as_deref(), Some("Kartons bestellen."));
        assert!(s.subtitle.as_deref().unwrap().contains("#merker"));
    }

    #[test]
    fn aktion_springt_nach_obsidian() {
        let u = Umgebung::neu("action");
        let files = vec![datei("Projekte/Haus.md", "- [ ] Dach \u{1F4C5} 2026-09-12\n")];
        let h = map_files(&quelle(), &konfig(), &files, &u.ctx());
        match h.signals[0].action.as_ref().unwrap() {
            Action::OpenNote { vault, path } => {
                assert_eq!(vault, "Privat");
                assert_eq!(path, "Projekte/Haus.md");
            }
            other => panic!("falsche Aktion: {other:?}"),
        }
    }

    #[test]
    fn read_tasks_aus_schaltet_aufgaben_ab() {
        let u = Umgebung::neu("notasks");
        let mut config = konfig();
        config.read_tasks = false;
        let files = vec![datei("A.md", "- [ ] Dach \u{1F4C5} 2026-09-12\n")];
        let h = map_files(&quelle(), &config, &files, &u.ctx());
        assert!(h.signals.is_empty());
        assert_eq!(h.overview.metric, "0");
    }

    #[test]
    fn gleiche_aufgabe_in_zwei_vaults_wird_entdoppelt() {
        let u = Umgebung::neu("dedup");
        let a = map_files(&quelle(), &konfig(), &[datei("A.md", "- [ ] Dach pruefen \u{1F4C5} 2026-09-12\n")], &u.ctx());
        let b = map_files(&quelle(), &konfig(), &[datei("B.md", "- [ ] Dach pruefen \u{1F4C5} 2026-09-12\n")], &u.ctx());
        assert_eq!(a.signals[0].dedup_key, b.signals[0].dedup_key);
    }

    #[test]
    fn liest_einen_echten_ordner() {
        let root = std::env::temp_dir().join(format!("tory-vault-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Projekte")).unwrap();
        std::fs::create_dir_all(root.join(".obsidian")).unwrap();
        std::fs::create_dir_all(root.join("Archiv")).unwrap();
        std::fs::write(root.join("Projekte/Haus.md"), "- [ ] Dach \u{1F4C5} 2026-09-12\n").unwrap();
        std::fs::write(root.join(".obsidian/workspace.md"), "- [ ] intern\n").unwrap();
        std::fs::write(root.join("Archiv/Alt.md"), "- [ ] alt \u{1F4C5} 2020-01-01\n").unwrap();
        std::fs::write(root.join("Notiz.txt"), "kein Markdown").unwrap();

        let files = read_local(&root, &konfig()).unwrap();
        let pfade: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(pfade, vec!["Projekte/Haus.md"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn fehlender_ordner_ist_ein_konfigurationsfehler() {
        let err = read_local(Path::new("/gibt/es/nicht"), &konfig()).unwrap_err();
        assert!(matches!(err.as_fault(), crate::model::SyncFault::Misconfigured { .. }));
    }

    #[test]
    fn scan_limit_greift() {
        let root = std::env::temp_dir().join(format!("tory-vault-limit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for i in 0..10 {
            std::fs::write(root.join(format!("N{i}.md")), "- [ ] x\n").unwrap();
        }
        let mut config = konfig();
        config.scan_limit = 4;
        assert_eq!(read_local(&root, &config).unwrap().len(), 4);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn propfind_wird_gelesen() {
        let xml = include_str!("../../tests/fixtures/webdav_propfind.xml");
        let entries = parse_propfind(xml, "https://cloud.example.de/remote.php/dav/files/jakob/Obsidian/Privat");
        let namen: Vec<(&str, bool)> = entries.iter().map(|e| (e.name.as_str(), e.is_dir)).collect();
        assert!(namen.contains(&("Projekte", true)), "Ordner erkannt: {namen:?}");
        assert!(namen.contains(&("Tagebuch 2026.md", false)), "Datei mit Leerzeichen: {namen:?}");
        assert!(namen.iter().any(|(n, _)| *n == "Notiz.md"));
        assert!(!namen.iter().any(|(n, _)| n.is_empty()), "der Ordner selbst ist heraus");
        let notiz = entries.iter().find(|e| e.name == "Notiz.md").unwrap();
        assert!(notiz.modified.is_some(), "getlastmodified gelesen");
    }

    #[test]
    fn http_datum_auch_mit_falschem_wochentag() {
        let richtig = parse_http_date("Fri, 11 Sep 2026 06:00:00 GMT").unwrap();
        // Derselbe Zeitpunkt, aber mit falsch gerechnetem Wochentag.
        let schief = parse_http_date("Mon, 11 Sep 2026 06:00:00 GMT").unwrap();
        assert_eq!(richtig, schief);
        assert_eq!(parse_http_date("gar kein Datum"), None);
    }

    #[test]
    fn pfade_werden_fuer_webdav_kodiert() {
        assert_eq!(encode_path("Projekte/Haus Nord.md"), "Projekte/Haus%20Nord.md");
        assert_eq!(encode_path("Notiz.md"), "Notiz.md", "Punkt bleibt stehen");
        assert_eq!(encode_path("Notiz-1_alt~.md"), "Notiz-1_alt~.md");
        assert_eq!(encode_path("Ordner/Über.md"), "Ordner/%C3%9Cber.md");
        assert_eq!(decode_path("Haus%20Nord"), "Haus Nord");
    }

    #[test]
    fn ausgeschlossene_ordner_werden_nicht_betreten() {
        let config = konfig();
        assert!(skip_dir(".obsidian", ".obsidian", &config));
        assert!(skip_dir("Archiv", "Archiv", &config));
        assert!(!skip_dir("Projekte", "Projekte", &config));
    }

    #[test]
    fn include_folders_beschraenkt_auf_einen_ast() {
        let mut config = konfig();
        config.include_folders = vec!["Projekte".into()];
        assert!(!skip_dir("Projekte", "Projekte", &config));
        assert!(skip_dir("Rezepte", "Rezepte", &config));
        assert!(included("Projekte/Haus.md", &config));
        assert!(!included("Rezepte/Brot.md", &config));
    }
}
