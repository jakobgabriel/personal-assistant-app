//! Die Konfiguration, die der Nutzer in der App bearbeitet.
//!
//! Zwei Dateien, streng getrennt:
//!
//! * `config.json` — alles Unkritische: URLs, Vault-Namen, Feed-Adressen,
//!   Takte, Reihenfolge der Karten. Lesbar, versionierbar, exportierbar.
//! * `secrets.bin` — Tokens und API-Schluessel, verschluesselt (siehe
//!   [`crate::secrets`]).
//!
//! Deshalb steht in dieser Datei nie ein Geheimnis, sondern nur der
//! *Schluesselname*, unter dem es im Secret-Store liegt.

use serde::{Deserialize, Serialize};

use crate::model::{SourceKind, SourceRef};

/// Wie oft eine Quelle abgefragt wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cadence {
    pub minutes: u32,
}

impl Cadence {
    pub const fn minutes(minutes: u32) -> Self {
        Self { minutes }
    }
}

/// Gemeinsame Felder jeder Quelleninstanz.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceCommon {
    /// Innerhalb der Art eindeutig; geht in [`SourceRef::instance`].
    pub instance: String,
    pub label: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub cadence: Cadence,
    /// Position in der Kartenliste des Startscreens.
    #[serde(default)]
    pub order: i32,
}

fn default_true() -> bool {
    true
}

impl SourceCommon {
    pub fn new(instance: &str, label: &str, cadence: Cadence) -> Self {
        Self {
            instance: instance.to_string(),
            label: label.to_string(),
            enabled: true,
            cadence,
            order: 0,
        }
    }
}

/// Wie Tory an die Dateien eines Vaults kommt.
///
/// Ein Vault ist ein Ordner mit Markdown. Auf dem Telefon liegt der selten
/// lokal, deshalb zwei Wege: der Ordner selbst (Desktop, oder ein per
/// Syncthing/Foldersync gespiegelter Pfad) oder WebDAV — Nextcloud, `rclone
/// serve webdav`, jeder WebDAV-Server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VaultAccess {
    Local {
        /// Absoluter Pfad zum Vault-Ordner.
        path: String,
    },
    Webdav {
        /// Basis-URL bis zum Vault-Ordner, etwa
        /// `https://cloud.example.de/remote.php/dav/files/jakob/Obsidian/Privat`.
        base_url: String,
        username: String,
        /// Name des Eintrags im Secret-Store, nicht das Passwort selbst.
        password_key: String,
    },
}

/// Ein Obsidian-Vault. Mehrere davon sind der Normalfall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObsidianSource {
    #[serde(flatten)]
    pub common: SourceCommon,
    /// Vault-Name wie in Obsidian — nur so funktioniert `obsidian://open?vault=`.
    pub vault_name: String,
    pub access: VaultAccess,
    /// Unterordner, die durchsucht werden. Leer = der ganze Vault.
    #[serde(default)]
    pub include_folders: Vec<String>,
    /// Ordner, die uebersprungen werden. `.obsidian` und `.trash` sind immer aus.
    #[serde(default = "default_vault_excludes")]
    pub exclude_folders: Vec<String>,
    /// Offene Checkboxen (`- [ ] …`) als Aufgabensignale lesen.
    #[serde(default = "default_true")]
    pub read_tasks: bool,
    /// Notizen mit diesem Tag im Frontmatter immer als Signal zeigen.
    #[serde(default)]
    pub pinned_tags: Vec<String>,
    /// Obergrenze gelesener Dateien pro Sync — WebDAV ist langsam.
    #[serde(default = "default_scan_limit")]
    pub scan_limit: usize,
}

fn default_vault_excludes() -> Vec<String> {
    vec![".obsidian".into(), ".trash".into(), "Archiv".into(), "Templates".into()]
}

fn default_scan_limit() -> usize {
    400
}

/// Der selbst gehostete Mindwtr-Cloud-Server. Die REST-Schnittstelle liegt
/// unter `/v1`, die Anmeldung ist ein Bearer-Token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MindwtrSource {
    #[serde(flatten)]
    pub common: SourceCommon,
    /// Basis-URL ohne `/v1`, etwa `https://mindwtr.example.de`.
    pub base_url: String,
    pub token_key: String,
    /// Nur diese Status als Signal anzeigen.
    #[serde(default = "default_mindwtr_statuses")]
    pub statuses: Vec<String>,
    /// Aufgaben ohne Datum ebenfalls zeigen (sonst nur Termine und Fristen).
    #[serde(default)]
    pub include_undated: bool,
    /// Tage im Voraus, ab wann eine Faelligkeit interessant wird.
    #[serde(default = "default_horizon_days")]
    pub horizon_days: i64,
}

fn default_mindwtr_statuses() -> Vec<String> {
    vec!["inbox".into(), "next".into(), "waiting".into()]
}

fn default_horizon_days() -> i64 {
    7
}

/// Eine NocoDB-Tabelle. Weil NocoDB beliebige Spalten hat, sagt eine Zuordnung,
/// welche Spalte Titel, Datum und Status ist — ohne sie koennte Tory die Zeilen
/// nicht in Signale uebersetzen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NocodbTableMap {
    pub table_id: String,
    pub label: String,
    /// Optional: eine gespeicherte Ansicht statt der ganzen Tabelle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_id: Option<String>,
    pub title_field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle_field: Option<String>,
    /// Spalte mit dem Datum, das die Dringlichkeit bestimmt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_field: Option<String>,
    /// Statuswerte, die als abgeschlossen gelten und kein Signal mehr erzeugen.
    #[serde(default)]
    pub done_values: Vec<String>,
    /// NocoDB-`where`-Ausdruck, etwa `(Status,neq,Abgelehnt)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(default = "default_row_limit")]
    pub limit: u32,
}

fn default_row_limit() -> u32 {
    100
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NocodbSource {
    #[serde(flatten)]
    pub common: SourceCommon,
    /// Basis-URL ohne `/api`, etwa `https://noco.example.de`.
    pub base_url: String,
    /// Name des `xc-token` im Secret-Store.
    pub token_key: String,
    pub tables: Vec<NocodbTableMap>,
}

/// Gmail, lesend. OAuth 2 mit PKCE, eigenes Google-Cloud-Projekt, Nutzer als
/// Testnutzer — dann ist keine App-Pruefung durch Google noetig.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GmailSource {
    #[serde(flatten)]
    pub common: SourceCommon,
    pub client_id: String,
    /// Muss mit dem Schema in `tauri.conf.json` uebereinstimmen.
    #[serde(default = "default_gmail_redirect")]
    pub redirect_uri: String,
    /// Gmail-Suchausdruecke; jeder wird eine eigene Gruppe von Signalen.
    #[serde(default = "default_gmail_queries")]
    pub queries: Vec<GmailQuery>,
    /// Obergrenze der Nachrichten pro Suchausdruck.
    #[serde(default = "default_gmail_limit")]
    pub per_query_limit: u32,
}

fn default_gmail_redirect() -> String {
    "de.tory.app://oauth2".into()
}

fn default_gmail_limit() -> u32 {
    15
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GmailQuery {
    pub label: String,
    /// Gmail-Suchsyntax, etwa `is:unread is:important newer_than:7d`.
    pub query: String,
    #[serde(default = "default_normal_urgency")]
    pub urgency: String,
}

fn default_normal_urgency() -> String {
    "normal".into()
}

fn default_gmail_queries() -> Vec<GmailQuery> {
    vec![
        GmailQuery {
            label: "Wichtig & ungelesen".into(),
            query: "is:unread is:important newer_than:14d".into(),
            urgency: "high".into(),
        },
        GmailQuery {
            label: "Direkt an mich".into(),
            query: "is:unread category:primary -is:important newer_than:3d".into(),
            urgency: "normal".into(),
        },
    ]
}

/// Wofuer ein Feed da ist. Trennt die drei Gruppen, die der Startscreen
/// unterschiedlich behandelt: Wetter oben als Kontextzeile, Lokales vor
/// Weltweitem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedTopic {
    Local,
    Global,
    Weather,
}

impl FeedTopic {
    pub fn label(self) -> &'static str {
        match self {
            FeedTopic::Local => "Regional",
            FeedTopic::Global => "Welt",
            FeedTopic::Weather => "Wetter",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feed {
    pub url: String,
    pub label: String,
    pub topic: FeedTopic,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Alle Feeds zusammen sind eine Quelleninstanz — eine Nachrichtenkarte, nicht
/// zwanzig.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedsSource {
    #[serde(flatten)]
    pub common: SourceCommon,
    pub feeds: Vec<Feed>,
    /// Schlagzeilen auf dem Startscreen, ueber alle Feeds.
    #[serde(default = "default_headline_limit")]
    pub headline_limit: usize,
    /// Aelteres wird nicht mehr gezeigt.
    #[serde(default = "default_max_age_hours")]
    pub max_age_hours: i64,
}

fn default_headline_limit() -> usize {
    10
}

fn default_max_age_hours() -> i64 {
    36
}

/// Welcher AI-Dienst angesprochen wird. Siehe `docs/roadmap-ki.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    Anthropic,
    OpenAi,
    /// Lokal oder im eigenen Netz, ohne Schluessel.
    Ollama,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default)]
    pub enabled: bool,
    pub provider: AiProviderKind,
    pub model: String,
    /// Nur fuer Ollama oder einen Proxy noetig.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Name des API-Schluessels im Secret-Store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_key: Option<String>,
    #[serde(default = "default_ai_max_tokens")]
    pub max_tokens: u32,
    /// Nur Titel und Zeiten an das Modell geben, keine Inhalte. Standard: an.
    #[serde(default = "default_true")]
    pub titles_only: bool,
}

fn default_ai_max_tokens() -> u32 {
    1024
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: AiProviderKind::Anthropic,
            model: "claude-sonnet-5".into(),
            base_url: None,
            api_key_key: None,
            max_tokens: default_ai_max_tokens(),
            titles_only: true,
        }
    }
}

/// Startscreen und Benachrichtigungen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardConfig {
    /// Wie viele Signale oben unter "Jetzt wichtig" stehen.
    #[serde(default = "default_top_signals")]
    pub top_signals: usize,
    /// Uhrzeit des Morgenbriefs, `HH:MM` lokal. Leer = aus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub morning_brief_at: Option<String>,
    /// Designrichtung: `cockpit` (dunkel, dicht) oder `calm` (hell, luftig).
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Reihenfolge der Karten als Quellen-Ids; Unbekanntes haengt hinten an.
    #[serde(default)]
    pub card_order: Vec<String>,
}

fn default_top_signals() -> usize {
    3
}

fn default_theme() -> String {
    "cockpit".into()
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            top_signals: default_top_signals(),
            morning_brief_at: Some("07:30".into()),
            theme: default_theme(),
            card_order: Vec::new(),
        }
    }
}

/// Die ganze Konfiguration. Eine Datei, exportierbar, ohne Geheimnisse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default = "default_config_version")]
    pub version: u32,
    #[serde(default)]
    pub dashboard: DashboardConfig,
    #[serde(default)]
    pub obsidian: Vec<ObsidianSource>,
    #[serde(default)]
    pub mindwtr: Vec<MindwtrSource>,
    #[serde(default)]
    pub nocodb: Vec<NocodbSource>,
    #[serde(default)]
    pub gmail: Vec<GmailSource>,
    #[serde(default)]
    pub feeds: Vec<FeedsSource>,
    #[serde(default)]
    pub ai: AiConfig,
}

fn default_config_version() -> u32 {
    1
}

impl Config {
    /// Alle konfigurierten Quellen als Verweis, in Kartenreihenfolge. Die
    /// Oberflaeche braucht die Liste auch fuer Quellen, die noch nie
    /// synchronisiert haben.
    pub fn sources(&self) -> Vec<SourceRef> {
        let mut refs: Vec<(i32, SourceRef)> = Vec::new();
        for s in &self.obsidian {
            refs.push((s.common.order, SourceRef::new(SourceKind::Obsidian, &s.common.instance, &s.common.label)));
        }
        for s in &self.mindwtr {
            refs.push((s.common.order, SourceRef::new(SourceKind::Mindwtr, &s.common.instance, &s.common.label)));
        }
        for s in &self.nocodb {
            refs.push((s.common.order, SourceRef::new(SourceKind::Nocodb, &s.common.instance, &s.common.label)));
        }
        for s in &self.gmail {
            refs.push((s.common.order, SourceRef::new(SourceKind::Gmail, &s.common.instance, &s.common.label)));
        }
        for s in &self.feeds {
            refs.push((s.common.order, SourceRef::new(SourceKind::Feeds, &s.common.instance, &s.common.label)));
        }
        refs.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id().cmp(&b.1.id())));
        refs.into_iter().map(|(_, r)| r).collect()
    }

    /// Namen aller Secret-Eintraege, auf die die Konfiguration verweist. Damit
    /// laesst sich der Store aufraeumen und pruefen, ob etwas fehlt.
    pub fn referenced_secret_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        for s in &self.obsidian {
            if let VaultAccess::Webdav { password_key, .. } = &s.access {
                keys.push(password_key.clone());
            }
        }
        keys.extend(self.mindwtr.iter().map(|s| s.token_key.clone()));
        keys.extend(self.nocodb.iter().map(|s| s.token_key.clone()));
        // Gmail legt Refresh-Token unter einem abgeleiteten Namen ab.
        keys.extend(self.gmail.iter().map(|s| crate::connectors::gmail::refresh_token_key(&s.common.instance)));
        if let Some(k) = &self.ai.api_key_key {
            keys.push(k.clone());
        }
        keys.sort();
        keys.dedup();
        keys
    }

    /// Fehler, die den Nutzer in den Einstellungen erwarten — statt erst beim
    /// Sync als Serverfehler aufzutauchen.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for r in self.sources() {
            if !seen.insert(r.id()) {
                problems.push(format!("Kennung doppelt vergeben: {}", r.id()));
            }
            if r.instance.trim().is_empty() {
                problems.push(format!("{}: Kennung ist leer", r.kind.label()));
            }
        }
        for s in &self.mindwtr {
            if !s.base_url.starts_with("http") {
                problems.push(format!("Mindwtr {}: URL braucht http:// oder https://", s.common.label));
            }
            if s.base_url.trim_end_matches('/').ends_with("/v1") {
                problems.push(format!(
                    "Mindwtr {}: die Basis-URL endet ohne /v1 — Tory haengt es selbst an",
                    s.common.label
                ));
            }
        }
        for s in &self.nocodb {
            if !s.base_url.starts_with("http") {
                problems.push(format!("NocoDB {}: URL braucht http:// oder https://", s.common.label));
            }
            for t in &s.tables {
                if t.title_field.trim().is_empty() {
                    problems.push(format!("NocoDB {} / {}: Titelspalte fehlt", s.common.label, t.label));
                }
            }
        }
        for s in &self.obsidian {
            if s.vault_name.trim().is_empty() {
                problems.push(format!("Obsidian {}: Vault-Name fehlt (sonst kein obsidian://-Sprung)", s.common.label));
            }
        }
        for s in &self.feeds {
            for f in &s.feeds {
                if !f.url.starts_with("http") {
                    problems.push(format!("Feed {}: URL braucht http:// oder https://", f.label));
                }
            }
        }
        if self.ai.enabled && self.ai.provider != AiProviderKind::Ollama && self.ai.api_key_key.is_none() {
            problems.push("AI ist an, aber kein API-Schluessel hinterlegt".into());
        }
        problems
    }
}
