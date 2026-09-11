//! Das gemeinsame Ereignismodell.
//!
//! Jede Quelle liefert ihre Inhalte als `Signal` und `Overview`. Die Oberflaeche
//! kennt nur diese zwei Typen — deshalb aendert eine neue Quelle den Startscreen
//! nicht. Die deutschen Begriffe aus `docs/schnittstellen.md` bilden sich so ab:
//! `Signal` -> [`Signal`], `Dringlichkeit` -> [`Urgency`], `QuellenId` ->
//! [`SourceRef`], `DomaenenUebersicht` -> [`Overview`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Art der angebundenen Quelle. Pro Art kann es mehrere Instanzen geben —
/// mehrere Obsidian-Vaults, mehrere NocoDB-Tabellen, mehrere Feed-Gruppen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Obsidian,
    Mindwtr,
    Nocodb,
    Gmail,
    Feeds,
}

impl SourceKind {
    pub fn slug(self) -> &'static str {
        match self {
            SourceKind::Obsidian => "obsidian",
            SourceKind::Mindwtr => "mindwtr",
            SourceKind::Nocodb => "nocodb",
            SourceKind::Gmail => "gmail",
            SourceKind::Feeds => "feeds",
        }
    }

    /// Anzeigename fuer die Oberflaeche.
    pub fn label(self) -> &'static str {
        match self {
            SourceKind::Obsidian => "Obsidian",
            SourceKind::Mindwtr => "Mindwtr",
            SourceKind::Nocodb => "NocoDB",
            SourceKind::Gmail => "Gmail",
            SourceKind::Feeds => "Nachrichten",
        }
    }
}

/// Verweis auf genau eine konfigurierte Quelleninstanz.
///
/// `instance` ist die vom Nutzer vergebene, innerhalb einer `kind` eindeutige
/// Kennung (etwa `privat` oder `arbeit` fuer zwei Vaults). `id()` bildet daraus
/// den stabilen Schluessel, unter dem Signale, Sync-Stand und Reihenfolge in der
/// Datenbank liegen.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceRef {
    pub kind: SourceKind,
    pub instance: String,
    /// Beschriftung fuer die Oberflaeche, etwa "Vault Privat".
    pub label: String,
}

impl SourceRef {
    pub fn new(kind: SourceKind, instance: impl Into<String>, label: impl Into<String>) -> Self {
        Self { kind, instance: instance.into(), label: label.into() }
    }

    /// Stabiler Schluessel, etwa `obsidian:privat`.
    pub fn id(&self) -> String {
        format!("{}:{}", self.kind.slug(), self.instance)
    }
}

/// Wie laut ein Signal ist. Bestimmt Farbe des Statusbalkens und Sortierung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    /// Ueberfaellig oder heute faellig mit Frist.
    Critical,
    /// Heute oder morgen relevant.
    High,
    /// Steht an, aber hat Zeit.
    Normal,
    /// Kontext ohne Handlungsbedarf — Schlagzeilen, Wetterlage.
    Info,
}

impl Urgency {
    /// Kleiner Wert = weiter oben. `Ord` liefert das schon, der Name macht die
    /// Sortierung an der Aufrufstelle lesbar.
    pub fn rank(self) -> u8 {
        match self {
            Urgency::Critical => 0,
            Urgency::High => 1,
            Urgency::Normal => 2,
            Urgency::Info => 3,
        }
    }
}

/// Wie `Signal::at` zu lesen ist. Ohne dieses Feld muesste die Oberflaeche pro
/// Domaene eine Sonderregel kennen, um aus einem Zeitstempel die richtige
/// Formulierung zu machen ("in 5 h" vs. "Frist in 1 T" vs. "vor 20 Min").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeKind {
    /// Fester Termin: "14:30".
    At,
    /// Faelligkeit oder Frist: "Frist in 1 T".
    Due,
    /// Spanne mit `window_end`: "14–16 Uhr".
    Window,
    /// Bereits geschehen: "vor 20 Min" — Mails, Schlagzeilen.
    Since,
}

/// Was beim Antippen passiert. Tory schreibt nach aussen nur dort, wo es
/// ausdruecklich vorgesehen ist (Mindwtr-Aufgabe abhaken); alles andere ist ein
/// Sprung in die Zielanwendung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// Im Systembrowser oeffnen.
    OpenUrl { url: String },
    /// `obsidian://open?vault=…&file=…` — oeffnet die Notiz in Obsidian.
    OpenNote { vault: String, path: String },
    /// Notiz in Tory selbst anzeigen.
    ShowNote { source: String, path: String },
    /// Mindwtr-Aufgabe abhaken. Der einzige schreibende Aufruf nach aussen.
    CompleteTask { source: String, task_id: String },
    /// Detailseite in Tory.
    OpenDetail { source: String, item_id: String },
}

/// Ein Ding mit Zeitpunkt, Frist oder Handlungsbedarf. Alles, was auf den
/// Startscreen darf, ist ein `Signal`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    /// Innerhalb der Quelle stabil — derselbe Gegenstand bekommt bei jedem Sync
    /// dieselbe Id, sonst verliert "stummgeschaltet bis" seinen Bezug.
    pub id: String,
    pub source: SourceRef,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Erste Zeilen des Inhalts. Nie der Volltext — dafuer gibt es die Quelle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_kind: Option<TimeKind>,
    /// Ende der Spanne bei [`TimeKind::Window`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_end: Option<DateTime<Utc>>,
    pub urgency: Urgency,
    /// Kurzes Etikett: Projektname, Absender, Tabellenname.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Action>,
    /// Darf in der Zeile abgehakt werden.
    #[serde(default)]
    pub completable: bool,
    /// Zusammenfuehrung quellenuebergreifend: dieselbe Nachricht aus zwei Feeds
    /// oder dieselbe Aufgabe in Obsidian und Mindwtr erscheint einmal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedup_key: Option<String>,
}

impl Signal {
    /// Minimales Signal; die weiteren Felder setzt der Aufrufer per `..`.
    pub fn new(source: SourceRef, id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            source,
            title: title.into(),
            subtitle: None,
            excerpt: None,
            at: None,
            time_kind: None,
            window_end: None,
            urgency: Urgency::Normal,
            badge: None,
            tags: Vec::new(),
            action: None,
            completable: false,
            dedup_key: None,
        }
    }

    /// Schluessel ueber Quelle und Signal-Id, eindeutig in der ganzen App.
    pub fn key(&self) -> String {
        format!("{}/{}", self.source.id(), self.id)
    }
}

/// Eine Zeile in der Domaenenkarte.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverviewLine {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub urgency: Option<Urgency>,
}

/// Karte (Richtung A), Kennzahl in der Leiste (B) und Kachel (C) aus einer
/// Struktur. Die Designrichtung entscheidet nur, welche Felder sie zeigt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overview {
    pub source: SourceRef,
    /// Bereits formatiert, etwa "12 offen".
    pub metric: String,
    /// Derselbe Wert als Zahl — fuer Schwellen und Sortierung.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_raw: Option<f64>,
    /// Was die Zahl bedeutet, etwa "Aufgaben".
    pub caption: String,
    /// Zusatz wie "3 ueberfaellig".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<OverviewLine>,
    /// Fortschritt 0.0–1.0, etwa erledigte Aufgaben eines Projekts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f32>,
}

impl Overview {
    pub fn empty(source: SourceRef, caption: impl Into<String>) -> Self {
        Self {
            source,
            metric: "—".into(),
            metric_raw: None,
            caption: caption.into(),
            note: None,
            lines: Vec::new(),
            progress: None,
        }
    }
}

/// Warum ein Sync nicht geklappt hat. Die Faelle sehen in der Oberflaeche
/// unterschiedlich aus, deshalb sind sie unterschiedliche Varianten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncFault {
    /// Stiller Graustich, kein Banner.
    Offline,
    /// Banner mit Knopf — Token abgelaufen, Nutzer muss handeln.
    AuthExpired { detail: String },
    /// Kontingent erschoepft; kein Wiederholungsversuch bis `retry_after`.
    RateLimited { retry_after: Option<DateTime<Utc>> },
    /// Fehlkonfiguration: falsche URL, unbekannte Tabelle.
    Misconfigured { detail: String },
    /// Server antwortet, aber falsch.
    Server { status: u16, detail: String },
    Unknown { detail: String },
}

impl SyncFault {
    /// Ein erneuter Versuch mit Backoff ist sinnvoll.
    pub fn retryable(&self) -> bool {
        matches!(self, SyncFault::Offline | SyncFault::Server { .. } | SyncFault::Unknown { .. })
    }

    pub fn message(&self) -> String {
        match self {
            SyncFault::Offline => "Kein Netz".into(),
            SyncFault::AuthExpired { detail } => format!("Anmeldung abgelaufen: {detail}"),
            SyncFault::RateLimited { .. } => "Kontingent erschoepft".into(),
            SyncFault::Misconfigured { detail } => format!("Konfiguration: {detail}"),
            SyncFault::Server { status, detail } => format!("Server {status}: {detail}"),
            SyncFault::Unknown { detail } => detail.clone(),
        }
    }
}

/// Aktualitaet einer Quelle. Haengt an jeder Uebersicht, damit die App nie alte
/// Zahlen als aktuell ausgibt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncState {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ok: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempt: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<SyncFault>,
    /// Aufeinanderfolgende Fehlversuche — Grundlage des Backoffs.
    #[serde(default)]
    pub failures: u32,
}

impl SyncState {
    pub fn fresh(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            last_ok: None,
            last_attempt: None,
            fault: None,
            failures: 0,
        }
    }
}

/// Was eine Quelle bei einem Sync abgeliefert hat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Harvest {
    pub source: SourceRef,
    pub signals: Vec<Signal>,
    pub overview: Overview,
}

impl Harvest {
    pub fn new(source: SourceRef, caption: impl Into<String>) -> Self {
        Self { overview: Overview::empty(source.clone(), caption), source, signals: Vec::new() }
    }
}
