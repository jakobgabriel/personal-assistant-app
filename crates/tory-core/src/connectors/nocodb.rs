//! NocoDB — selbst gehostete Datenbank mit Tabellenoberflaeche.
//!
//! NocoDB hat keine feste Bedeutung fuer seine Spalten: eine Tabelle kann
//! Bewerbungen, Wartungstermine oder Vertraege enthalten. Deshalb bringt jede
//! angebundene Tabelle eine [`NocodbTableMap`] mit, die sagt, welche Spalte
//! Titel, Datum und Status ist. Ohne diese Zuordnung koennte Tory die Zeilen
//! nicht in Signale uebersetzen — und mit ihr braucht es keinen Code pro
//! Tabelle.
//!
//! Genutzte Endpunkte (API v2, Anmeldung per `xc-token`):
//!
//! | Zweck | Aufruf |
//! | --- | --- |
//! | Zeilen lesen | `GET /api/v2/tables/{tableId}/records?limit=&where=&viewId=` |
//! | Tabellen einer Base | `GET /api/v2/meta/bases/{baseId}/tables` |

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde::Deserialize;
use serde_json::Value;

use crate::config::{NocodbSource, NocodbTableMap};
use crate::error::Result;
use crate::http::{expect_ok, join};
use crate::model::{
    Action, Harvest, Overview, OverviewLine, Signal, SourceKind, SourceRef, TimeKind, Urgency,
};

use super::{shorten, urgency_for_due, Connector, SyncContext};

#[derive(Debug, Deserialize)]
struct RecordPage {
    #[serde(default)]
    list: Vec<Value>,
}

/// Eine Tabelle, wie die Meta-API sie beschreibt — fuer den Einrichtungsdialog,
/// damit der Nutzer Tabellen-Ids nicht abschreiben muss.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct TableInfo {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
struct TableList {
    #[serde(default)]
    list: Vec<TableInfo>,
}

/// Eine Base (Projekt) in NocoDB.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct BaseInfo {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
struct BaseList {
    #[serde(default)]
    list: Vec<BaseInfo>,
}

pub struct NocodbConnector {
    source: SourceRef,
    config: NocodbSource,
}

impl NocodbConnector {
    pub fn new(config: NocodbSource) -> Self {
        let source = SourceRef::new(SourceKind::Nocodb, &config.common.instance, &config.common.label);
        Self { source, config }
    }
}

#[async_trait]
impl Connector for NocodbConnector {
    fn source(&self) -> &SourceRef {
        &self.source
    }

    async fn fetch(&self, ctx: &SyncContext<'_>) -> Result<Harvest> {
        let token = ctx.secrets.require(&self.config.token_key)?;
        let mut per_table = Vec::new();
        for table in &self.config.tables {
            let rows = fetch_rows(ctx, &self.config.base_url, token, table).await?;
            per_table.push((table.clone(), rows));
        }
        Ok(map_rows(&self.source, &per_table, ctx))
    }
}

async fn fetch_rows(
    ctx: &SyncContext<'_>,
    base_url: &str,
    token: &str,
    table: &NocodbTableMap,
) -> Result<Vec<Value>> {
    let url = join(base_url, &format!("api/v2/tables/{}/records", table.table_id));
    let limit = table.limit.clamp(1, 1000).to_string();
    let mut query: Vec<(&str, String)> = vec![("limit", limit)];
    if let Some(view) = &table.view_id {
        query.push(("viewId", view.clone()));
    }
    if let Some(filter) = &table.filter {
        query.push(("where", filter.clone()));
    }
    if let Some(field) = &table.date_field {
        // Nach dem Datum sortiert kommen die dringenden Zeilen zuerst — wichtig,
        // weil `limit` sonst willkuerlich abschneidet.
        query.push(("sort", field.clone()));
    }
    let page: RecordPage =
        expect_ok(ctx.http.get(url).header("xc-token", token).query(&query).send().await?)
            .await?
            .json()
            .await?;
    Ok(page.list)
}

/// Listet die Bases — fuer den Einrichtungsdialog.
pub async fn list_bases(ctx: &SyncContext<'_>, base_url: &str, token: &str) -> Result<Vec<BaseInfo>> {
    let url = join(base_url, "api/v2/meta/bases");
    Ok(expect_ok(ctx.http.get(url).header("xc-token", token).send().await?)
        .await?
        .json::<BaseList>()
        .await?
        .list)
}

/// Listet die Tabellen einer Base — fuer den Einrichtungsdialog.
pub async fn list_tables(
    ctx: &SyncContext<'_>,
    base_url: &str,
    token: &str,
    base_id: &str,
) -> Result<Vec<TableInfo>> {
    let url = join(base_url, &format!("api/v2/meta/bases/{base_id}/tables"));
    Ok(expect_ok(ctx.http.get(url).header("xc-token", token).send().await?)
        .await?
        .json::<TableList>()
        .await?
        .list)
}

/// Zeilen -> Signale und Karte. Reine Funktion, gegen Fixtures getestet.
pub fn map_rows(
    source: &SourceRef,
    per_table: &[(NocodbTableMap, Vec<Value>)],
    ctx: &SyncContext<'_>,
) -> Harvest {
    let today = ctx.today();
    let mut signals = Vec::new();
    let mut lines = Vec::new();
    let mut open_total = 0usize;
    let mut overdue_total = 0usize;

    for (table, rows) in per_table {
        let mut open = 0usize;
        let mut overdue = 0usize;
        for row in rows {
            let Some(title) = field_text(row, &table.title_field) else { continue };
            if title.trim().is_empty() {
                continue;
            }
            let status = table.status_field.as_deref().and_then(|f| field_text(row, f));
            let done = status
                .as_deref()
                .is_some_and(|s| table.done_values.iter().any(|d| d.eq_ignore_ascii_case(s)));
            if done {
                continue;
            }
            open += 1;

            let due = table.date_field.as_deref().and_then(|f| field_date(row, f));
            if due.is_some_and(|d| d < today) {
                overdue += 1;
            }
            let urgency = due.map(|d| urgency_for_due(d, today)).unwrap_or(Urgency::Info);

            let row_id = row_id(row).unwrap_or_else(|| format!("h{}", stable_hash(&title)));
            let mut subtitle_parts = vec![table.label.clone()];
            if let Some(s) = &status {
                subtitle_parts.push(s.clone());
            }

            signals.push(Signal {
                subtitle: Some(subtitle_parts.join(" · ")),
                excerpt: table
                    .subtitle_field
                    .as_deref()
                    .and_then(|f| field_text(row, f))
                    .map(|t| shorten(&t, 140)),
                at: due.map(|d| ctx.local_date_start(d)),
                time_kind: due.map(|_| TimeKind::Due),
                urgency,
                badge: Some(table.label.clone()),
                action: Some(Action::OpenDetail {
                    source: source.id(),
                    item_id: format!("{}:{}", table.table_id, row_id),
                }),
                dedup_key: None,
                ..Signal::new(source.clone(), format!("{}:{}", table.table_id, row_id), title)
            });
        }
        open_total += open;
        overdue_total += overdue;
        lines.push(OverviewLine {
            text: format!("{}: {open}", table.label),
            note: (overdue > 0).then(|| format!("{overdue} ueberfaellig")),
            urgency: (overdue > 0).then_some(Urgency::Critical),
        });
    }

    Harvest {
        overview: Overview {
            source: source.clone(),
            metric: open_total.to_string(),
            metric_raw: Some(open_total as f64),
            caption: "Eintraege offen".into(),
            note: (overdue_total > 0).then(|| format!("{overdue_total} ueberfaellig")),
            lines,
            progress: None,
        },
        source: source.clone(),
        signals,
    }
}

pub fn token_key(instance: &str) -> String {
    format!("nocodb.{instance}.token")
}

/// NocoDB nennt die Id je nach Tabelle `Id` oder `id`; Ansichten liefern
/// zusaetzlich `ncRecordId`.
fn row_id(row: &Value) -> Option<String> {
    for key in ["Id", "id", "ncRecordId", "ID"] {
        match row.get(key) {
            Some(Value::Number(n)) => return Some(n.to_string()),
            Some(Value::String(s)) if !s.is_empty() => return Some(s.clone()),
            _ => {}
        }
    }
    None
}

/// Liest eine Spalte als Text. NocoDB liefert je nach Spaltentyp Zahl, Text,
/// Auswahlobjekt oder Liste — alles muss zu einer Zeile werden.
fn field_text(row: &Value, field: &str) -> Option<String> {
    let value = row.get(field)?;
    flatten(value)
}

fn flatten(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => (!s.is_empty()).then(|| s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(if *b { "ja".into() } else { "nein".into() }),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().filter_map(flatten).collect();
            (!parts.is_empty()).then(|| parts.join(", "))
        }
        // Verknuepfte Zeilen und Auswahlfelder kommen als Objekt; NocoDB legt
        // den lesbaren Wert unter `title` bzw. `value` ab.
        Value::Object(map) => map
            .get("title")
            .or_else(|| map.get("value"))
            .or_else(|| map.get("name"))
            .and_then(flatten),
    }
}

/// Liest eine Spalte als Datum. NocoDB schreibt Datumsfelder als `2026-09-12`,
/// Datum-Zeit-Felder als `2026-09-12 08:00:00+00:00` oder RFC 3339.
fn field_date(row: &Value, field: &str) -> Option<NaiveDate> {
    let raw = field_text(row, field)?;
    let raw = raw.trim();
    if let Ok(d) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Some(d);
    }
    if let Ok(t) = DateTime::parse_from_rfc3339(raw) {
        return Some(t.naive_utc().date());
    }
    if let Ok(t) = DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%:z") {
        return Some(t.naive_utc().date());
    }
    if let Ok(t) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return Some(t.date());
    }
    // Deutsche Schreibweise, weil NocoDB sie in manchen Ansichten ausgibt.
    NaiveDate::parse_from_str(raw, "%d.%m.%Y").ok()
}

fn stable_hash(input: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(input.as_bytes()).iter().take(6).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Cadence, SourceCommon};
    use crate::secrets::SecretStore;
    use chrono::{TimeZone, Utc};

    fn quelle() -> SourceRef {
        SourceRef::new(SourceKind::Nocodb, "haupt", "NocoDB")
    }

    fn bewerbungen() -> NocodbTableMap {
        NocodbTableMap {
            table_id: "mtbl_bewerbungen".into(),
            label: "Bewerbungen".into(),
            view_id: None,
            title_field: "Position".into(),
            subtitle_field: Some("Notiz".into()),
            date_field: Some("Frist".into()),
            status_field: Some("Status".into()),
            done_values: vec!["Abgelehnt".into(), "Zurueckgezogen".into()],
            filter: None,
            limit: 100,
        }
    }

    struct Umgebung {
        dir: std::path::PathBuf,
        secrets: SecretStore,
        http: reqwest::Client,
    }

    impl Umgebung {
        fn neu(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("tory-noco-{name}-{}", std::process::id()));
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

    fn zeilen() -> Vec<Value> {
        let page: RecordPage =
            serde_json::from_str(include_str!("../../tests/fixtures/nocodb_records.json")).unwrap();
        page.list
    }

    fn konfig() -> NocodbSource {
        NocodbSource {
            common: SourceCommon::new("haupt", "NocoDB", Cadence::minutes(60)),
            base_url: "https://noco.example.de".into(),
            token_key: token_key("haupt"),
            tables: vec![bewerbungen()],
        }
    }

    #[test]
    fn zeilen_werden_zu_signalen() {
        let u = Umgebung::neu("map");
        let h = map_rows(&quelle(), &[(bewerbungen(), zeilen())], &u.ctx());
        // Vier Zeilen, eine mit Status "Abgelehnt" fliegt heraus.
        assert_eq!(h.signals.len(), 3);
        assert_eq!(h.overview.metric, "3");
        assert!(!h.signals.iter().any(|s| s.title.contains("Werkstudent")));
    }

    #[test]
    fn frist_bestimmt_dringlichkeit() {
        let u = Umgebung::neu("due");
        let h = map_rows(&quelle(), &[(bewerbungen(), zeilen())], &u.ctx());
        let ueberfaellig = h.signals.iter().find(|s| s.title.contains("Data Engineer")).unwrap();
        assert_eq!(ueberfaellig.urgency, Urgency::Critical);
        let spaeter = h.signals.iter().find(|s| s.title.contains("Lean Manager")).unwrap();
        assert_eq!(spaeter.urgency, Urgency::Info);
    }

    #[test]
    fn ohne_datumsspalte_bleibt_alles_info() {
        let u = Umgebung::neu("nodate");
        let mut table = bewerbungen();
        table.date_field = None;
        let h = map_rows(&quelle(), &[(table, zeilen())], &u.ctx());
        assert!(h.signals.iter().all(|s| s.urgency == Urgency::Info));
        assert!(h.signals.iter().all(|s| s.at.is_none()));
    }

    #[test]
    fn verknuepfte_und_auswahlfelder_werden_lesbar() {
        let u = Umgebung::neu("linked");
        let mut table = bewerbungen();
        table.subtitle_field = Some("Firma".into());
        let h = map_rows(&quelle(), &[(table, zeilen())], &u.ctx());
        let s = h.signals.iter().find(|s| s.title.contains("IIoT")).unwrap();
        assert_eq!(s.excerpt.as_deref(), Some("Beispiel GmbH"), "Objekt mit title: …");
    }

    #[test]
    fn karte_zaehlt_pro_tabelle() {
        let u = Umgebung::neu("lines");
        let h = map_rows(&quelle(), &[(bewerbungen(), zeilen())], &u.ctx());
        assert_eq!(h.overview.lines.len(), 1);
        assert_eq!(h.overview.lines[0].text, "Bewerbungen: 3");
        assert_eq!(h.overview.lines[0].note.as_deref(), Some("1 ueberfaellig"));
    }

    #[test]
    fn signal_id_enthaelt_tabelle_und_zeile() {
        let u = Umgebung::neu("id");
        let h = map_rows(&quelle(), &[(bewerbungen(), zeilen())], &u.ctx());
        assert!(h.signals.iter().all(|s| s.id.starts_with("mtbl_bewerbungen:")));
    }

    #[test]
    fn datumsformate() {
        let row = serde_json::json!({
            "a": "2026-09-12",
            "b": "2026-09-12 08:00:00+00:00",
            "c": "2026-09-12T08:00:00Z",
            "d": "12.09.2026",
            "e": "kein Datum"
        });
        for key in ["a", "b", "c", "d"] {
            assert_eq!(field_date(&row, key), NaiveDate::from_ymd_opt(2026, 9, 12), "Feld {key}");
        }
        assert_eq!(field_date(&row, "e"), None);
    }

    #[test]
    fn mehrere_tabellen_landen_in_einer_karte() {
        let u = Umgebung::neu("multi");
        let mut zweite = bewerbungen();
        zweite.table_id = "mtbl_wartung".into();
        zweite.label = "Wartung".into();
        let h = map_rows(&quelle(), &[(bewerbungen(), zeilen()), (zweite, zeilen())], &u.ctx());
        assert_eq!(h.overview.lines.len(), 2);
        assert_eq!(h.overview.metric, "6");
        assert_eq!(konfig().tables.len(), 1);
    }
}
