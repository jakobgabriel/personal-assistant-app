//! Gmail, lesend.
//!
//! Anmeldung: OAuth 2 mit PKCE und **ohne Client-Secret**. Eine App, die auf dem
//! Telefon liegt, kann kein Geheimnis huegten — Google nennt das einen
//! "installed app"-Client, fuer den PKCE genau deshalb gedacht ist. Der
//! Rueckkanal ist ein eigenes URL-Schema (`de.tory.app://oauth2`), das die
//! Tauri-App per Deep Link entgegennimmt; auf dem Desktop funktioniert dasselbe
//! Schema ueber die Systemregistrierung.
//!
//! Umfang: `gmail.readonly`. Tory schreibt keine Mails und markiert nichts als
//! gelesen — angetippt oeffnet sich die Nachricht in Gmail.
//!
//! Genutzte Endpunkte:
//!
//! | Zweck | Aufruf |
//! | --- | --- |
//! | Suchen | `GET /gmail/v1/users/me/messages?q=…&maxResults=…` |
//! | Kopfdaten | `GET /gmail/v1/users/me/messages/{id}?format=metadata` |
//! | Token tauschen/erneuern | `POST https://oauth2.googleapis.com/token` |
//!
//! Gelesen werden nur Kopfzeilen (`From`, `Subject`, `Date`) und der von Google
//! gelieferte `snippet`. Kein Nachrichtentext, keine Anhaenge.

use async_trait::async_trait;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::config::{GmailQuery, GmailSource};
use crate::error::{Error, Result};
use crate::model::{
    Action, Harvest, Overview, OverviewLine, Signal, SourceKind, SourceRef, SyncFault, TimeKind,
    Urgency,
};
use crate::secrets::SecretStore;

use super::{shorten, Connector, SyncContext};

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const API_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
pub const SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";

/// Der Schluesselname, unter dem das Refresh-Token liegt.
pub fn refresh_token_key(instance: &str) -> String {
    format!("gmail.{instance}.refresh_token")
}

/// Was fuer einen Anmeldeversuch gemerkt werden muss, bis der Rueckkanal kommt.
#[derive(Debug, Clone)]
pub struct AuthStart {
    /// Diese URL oeffnet die App im Systembrowser.
    pub url: String,
    /// Muss bis zum Tausch aufbewahrt werden.
    pub code_verifier: String,
    /// Schutz gegen untergeschobene Rueckleitungen.
    pub state: String,
}

/// Baut die Anmelde-URL samt PKCE-Paar.
pub fn begin_auth(client_id: &str, redirect_uri: &str) -> AuthStart {
    let code_verifier = random_token(64);
    let challenge = code_challenge(&code_verifier);
    let state = random_token(16);
    let url = format!(
        "{AUTH_ENDPOINT}?client_id={}&redirect_uri={}&response_type=code&scope={}\
         &code_challenge={challenge}&code_challenge_method=S256&state={state}\
         &access_type=offline&prompt=consent",
        urlencode(client_id),
        urlencode(redirect_uri),
        urlencode(SCOPE),
    );
    AuthStart { url, code_verifier, state }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Tauscht den Code aus dem Rueckkanal gegen Tokens und legt das Refresh-Token ab.
///
/// `access_type=offline` plus `prompt=consent` sorgt dafuer, dass Google ein
/// Refresh-Token mitschickt; ohne `prompt=consent` kommt beim zweiten Mal keines,
/// und die App muesste bei jedem Start neu anmelden.
pub async fn finish_auth(
    http: &reqwest::Client,
    secrets: &mut SecretStore,
    instance: &str,
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    code_verifier: &str,
) -> Result<()> {
    let response = crate::http::expect_ok(
        http.post(TOKEN_ENDPOINT)
            .form(&[
                ("client_id", client_id),
                ("code", code),
                ("code_verifier", code_verifier),
                ("grant_type", "authorization_code"),
                ("redirect_uri", redirect_uri),
            ])
            .send()
            .await?,
    )
    .await?;
    let token: TokenResponse = response.json().await?;
    let refresh = token.refresh_token.ok_or_else(|| {
        Error::config(
            "Google hat kein Refresh-Token geliefert. Zugriff unter \
             myaccount.google.com/permissions entfernen und erneut anmelden.",
        )
    })?;
    secrets.set(&refresh_token_key(instance), &refresh)?;
    Ok(())
}

/// Holt ein frisches Access-Token. Access-Tokens leben eine Stunde, deshalb wird
/// bei jedem Sync eines geholt statt eines zwischengespeichert — das spart die
/// Ablauflogik und kostet einen Aufruf.
async fn access_token(
    ctx: &SyncContext<'_>,
    instance: &str,
    client_id: &str,
) -> Result<(String, Option<i64>)> {
    let refresh = ctx.secrets.get(&refresh_token_key(instance)).ok_or_else(|| {
        Error::Sync(SyncFault::AuthExpired { detail: "Noch nicht bei Google angemeldet".into() })
    })?;
    let response = ctx
        .http
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("client_id", client_id),
            ("refresh_token", refresh),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::BAD_REQUEST {
        // Google antwortet 400 `invalid_grant`, wenn der Nutzer den Zugriff
        // entzogen hat. Das ist kein Serverfehler, sondern ein Fall fuer das
        // Banner mit Knopf.
        let detail = response.text().await.unwrap_or_default();
        return Err(Error::Sync(SyncFault::AuthExpired {
            detail: shorten(&detail, 160),
        }));
    }
    let token: TokenResponse = crate::http::expect_ok(response).await?.json().await?;
    Ok((token.access_token, token.expires_in))
}

#[derive(Debug, Deserialize)]
struct MessageList {
    #[serde(default)]
    messages: Vec<MessageId>,
    #[serde(default, rename = "resultSizeEstimate")]
    result_size_estimate: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct MessageId {
    id: String,
}

/// Eine Nachricht in der Form, die `format=metadata` liefert.
#[derive(Debug, Clone, Deserialize)]
pub struct GmailMessage {
    pub id: String,
    #[serde(default, rename = "threadId")]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub snippet: Option<String>,
    #[serde(default, rename = "labelIds")]
    pub label_ids: Vec<String>,
    #[serde(default, rename = "internalDate")]
    pub internal_date: Option<String>,
    #[serde(default)]
    pub payload: Option<MessagePayload>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessagePayload {
    #[serde(default)]
    pub headers: Vec<Header>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

impl GmailMessage {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.payload.as_ref()?.headers.iter().find(|h| h.name.eq_ignore_ascii_case(name)).map(|h| h.value.as_str())
    }

    /// Zeitpunkt der Nachricht. `internalDate` ist Millisekunden seit Epoche.
    pub fn received(&self) -> Option<DateTime<Utc>> {
        let millis: i64 = self.internal_date.as_deref()?.parse().ok()?;
        DateTime::from_timestamp_millis(millis)
    }

    pub fn unread(&self) -> bool {
        self.label_ids.iter().any(|l| l == "UNREAD")
    }
}

pub struct GmailConnector {
    source: SourceRef,
    config: GmailSource,
}

impl GmailConnector {
    pub fn new(config: GmailSource) -> Self {
        let source = SourceRef::new(SourceKind::Gmail, &config.common.instance, &config.common.label);
        Self { source, config }
    }
}

#[async_trait]
impl Connector for GmailConnector {
    fn source(&self) -> &SourceRef {
        &self.source
    }

    async fn fetch(&self, ctx: &SyncContext<'_>) -> Result<Harvest> {
        let (token, _) =
            access_token(ctx, &self.config.common.instance, &self.config.client_id).await?;
        let mut groups = Vec::new();

        for query in &self.config.queries {
            let list: MessageList = crate::http::expect_ok(
                ctx.http
                    .get(format!("{API_BASE}/messages"))
                    .bearer_auth(&token)
                    .query(&[
                        ("q", query.query.as_str()),
                        ("maxResults", &self.config.per_query_limit.to_string()),
                    ])
                    .send()
                    .await?,
            )
            .await?
            .json()
            .await?;

            let estimate = list.result_size_estimate.unwrap_or(list.messages.len() as u32);
            let mut messages = Vec::new();
            for id in &list.messages {
                // `format=metadata` mit ausgewaehlten Kopfzeilen: die Antwort
                // bleibt klein und enthaelt keinen Nachrichtentext.
                let message: GmailMessage = crate::http::expect_ok(
                    ctx.http
                        .get(format!("{API_BASE}/messages/{}", id.id))
                        .bearer_auth(&token)
                        .query(&[
                            ("format", "metadata"),
                            ("metadataHeaders", "From"),
                            ("metadataHeaders", "Subject"),
                            ("metadataHeaders", "List-Id"),
                        ])
                        .send()
                        .await?,
                )
                .await?
                .json()
                .await?;
                messages.push(message);
            }
            groups.push((query.clone(), messages, estimate));
        }

        Ok(map_messages(&self.source, &groups, ctx.now))
    }
}

/// Nachrichten -> Signale und Karte. Reine Funktion, gegen Fixtures getestet.
pub fn map_messages(
    source: &SourceRef,
    groups: &[(GmailQuery, Vec<GmailMessage>, u32)],
    now: DateTime<Utc>,
) -> Harvest {
    let mut signals = Vec::new();
    let mut lines = Vec::new();
    let mut unread_total = 0usize;

    for (query, messages, estimate) in groups {
        let urgency = parse_urgency(&query.urgency);
        let unread = messages.iter().filter(|m| m.unread()).count();
        unread_total += unread;

        lines.push(OverviewLine {
            text: format!("{}: {}", query.label, estimate),
            note: (*estimate as usize > messages.len())
                .then(|| format!("{} geladen", messages.len())),
            urgency: Some(urgency),
        });

        for message in messages {
            let subject = message.header("Subject").unwrap_or("(kein Betreff)").trim();
            let from = message.header("From").map(sender_name);
            let newsletter = message.header("List-Id").is_some();
            let received = message.received();

            signals.push(Signal {
                subtitle: Some(match (&from, newsletter) {
                    (Some(name), true) => format!("{name} · Newsletter"),
                    (Some(name), false) => name.clone(),
                    (None, _) => query.label.clone(),
                }),
                excerpt: message.snippet.as_deref().map(|s| shorten(s, 160)).filter(|s| !s.is_empty()),
                at: received,
                time_kind: received.map(|_| TimeKind::Since),
                // Newsletter sind nie dringend, egal wie die Suche heisst.
                urgency: if newsletter { Urgency::Info } else { urgency },
                badge: Some(query.label.clone()),
                action: Some(Action::OpenUrl {
                    url: format!(
                        "https://mail.google.com/mail/u/0/#inbox/{}",
                        message.thread_id.as_deref().unwrap_or(&message.id)
                    ),
                }),
                dedup_key: Some(format!("mail:{}", message.thread_id.as_deref().unwrap_or(&message.id))),
                ..Signal::new(
                    source.clone(),
                    message.id.clone(),
                    if subject.is_empty() { "(kein Betreff)".to_string() } else { subject.to_string() },
                )
            });
        }
    }

    // Innerhalb von Gmail: neueste zuerst. Die Gesamtsortierung macht der Store.
    signals.sort_by(|a, b| b.at.cmp(&a.at));
    let _ = now;

    Harvest {
        overview: Overview {
            source: source.clone(),
            metric: unread_total.to_string(),
            metric_raw: Some(unread_total as f64),
            caption: "ungelesen".into(),
            note: None,
            lines,
            progress: None,
        },
        source: source.clone(),
        signals,
    }
}

fn parse_urgency(raw: &str) -> Urgency {
    match raw {
        "critical" => Urgency::Critical,
        "high" => Urgency::High,
        "info" => Urgency::Info,
        _ => Urgency::Normal,
    }
}

/// `"Vorname Nachname" <a@b.de>` -> `Vorname Nachname`; nur eine Adresse ->
/// der Teil vor dem `@`.
fn sender_name(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(end) = raw.rfind('<') {
        let name = raw[..end].trim().trim_matches('"').trim();
        if !name.is_empty() {
            return name.to_string();
        }
        let address = raw[end + 1..].trim_end_matches('>');
        return address.split('@').next().unwrap_or(address).to_string();
    }
    raw.split('@').next().unwrap_or(raw).to_string()
}

fn random_token(bytes: usize) -> String {
    use rand::RngCore;
    let mut raw = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
}

fn code_challenge(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Kodiert einen Wert fuer den Query-Teil einer URL.
///
/// `NON_ALPHANUMERIC` waere zu grob: es kodiert auch `-._~`, die nach RFC 3986
/// unreserviert sind. Ein `redirect_uri` als `de%2Etory%2Eapp` wird von Google
/// zwar akzeptiert, weicht aber vom eingetragenen Wert ab — das ist genau die
/// Art Abweichung, die bei einem exakten Vergleich zu `redirect_uri_mismatch`
/// fuehrt.
const QUERY_VALUE: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

fn urlencode(raw: &str) -> String {
    percent_encoding::utf8_percent_encode(raw, QUERY_VALUE).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn quelle() -> SourceRef {
        SourceRef::new(SourceKind::Gmail, "privat", "Gmail")
    }

    fn nachrichten() -> Vec<GmailMessage> {
        let raw = include_str!("../../tests/fixtures/gmail_messages.json");
        serde_json::from_str(raw).unwrap()
    }

    fn suche(label: &str, urgency: &str) -> GmailQuery {
        GmailQuery { label: label.into(), query: "is:unread".into(), urgency: urgency.into() }
    }

    #[test]
    fn liest_die_echte_antwortform() {
        let messages = nachrichten();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].header("Subject"), Some("Rechnung 2026-0912 faellig"));
        assert_eq!(messages[0].header("From"), Some("\"Stadtwerke Nord\" <rechnung@stadtwerke.example>"));
        assert!(messages[0].unread());
        assert!(messages[0].received().is_some());
    }

    #[test]
    fn betreff_und_absender_werden_zur_signalzeile() {
        let h = map_messages(
            &quelle(),
            &[(suche("Wichtig", "high"), nachrichten(), 3)],
            Utc.with_ymd_and_hms(2026, 9, 11, 9, 0, 0).unwrap(),
        );
        let s = h.signals.iter().find(|s| s.title.starts_with("Rechnung")).unwrap();
        assert_eq!(s.subtitle.as_deref(), Some("Stadtwerke Nord"));
        assert_eq!(s.urgency, Urgency::High);
        assert!(s.excerpt.as_deref().unwrap().contains("Bitte begleichen"));
    }

    #[test]
    fn newsletter_bleibt_ruhig_auch_in_einer_wichtigen_suche() {
        let h = map_messages(
            &quelle(),
            &[(suche("Wichtig", "critical"), nachrichten(), 3)],
            Utc.with_ymd_and_hms(2026, 9, 11, 9, 0, 0).unwrap(),
        );
        let news = h.signals.iter().find(|s| s.title.contains("Wochenrueckblick")).unwrap();
        assert_eq!(news.urgency, Urgency::Info, "List-Id erkannt");
        assert!(news.subtitle.as_deref().unwrap().ends_with("Newsletter"));
    }

    #[test]
    fn aktion_zeigt_auf_den_thread() {
        let h = map_messages(
            &quelle(),
            &[(suche("Wichtig", "high"), nachrichten(), 3)],
            Utc.with_ymd_and_hms(2026, 9, 11, 9, 0, 0).unwrap(),
        );
        let s = h.signals.iter().find(|s| s.title.starts_with("Rechnung")).unwrap();
        match s.action.as_ref().unwrap() {
            Action::OpenUrl { url } => assert!(url.ends_with("/thread-1"), "{url}"),
            other => panic!("falsche Aktion: {other:?}"),
        }
    }

    #[test]
    fn karte_zaehlt_ungelesene_und_nennt_die_schaetzung() {
        let h = map_messages(
            &quelle(),
            &[(suche("Wichtig", "high"), nachrichten(), 42)],
            Utc.with_ymd_and_hms(2026, 9, 11, 9, 0, 0).unwrap(),
        );
        assert_eq!(h.overview.metric, "2", "eine der drei ist gelesen");
        assert_eq!(h.overview.lines[0].text, "Wichtig: 42");
        assert_eq!(h.overview.lines[0].note.as_deref(), Some("3 geladen"));
    }

    #[test]
    fn neueste_zuerst() {
        let h = map_messages(
            &quelle(),
            &[(suche("Wichtig", "high"), nachrichten(), 3)],
            Utc.with_ymd_and_hms(2026, 9, 11, 9, 0, 0).unwrap(),
        );
        let zeiten: Vec<_> = h.signals.iter().map(|s| s.at).collect();
        assert!(zeiten.windows(2).all(|w| w[0] >= w[1]), "{zeiten:?}");
    }

    #[test]
    fn absendernamen() {
        assert_eq!(sender_name("\"Stadtwerke Nord\" <a@b.de>"), "Stadtwerke Nord");
        assert_eq!(sender_name("Jakob Gabriel <a@b.de>"), "Jakob Gabriel");
        assert_eq!(sender_name("<info@example.de>"), "info");
        assert_eq!(sender_name("info@example.de"), "info");
    }

    #[test]
    fn pkce_paar_ist_gueltig() {
        let start = begin_auth("123.apps.googleusercontent.com", "de.tory.app://oauth2");
        assert!(start.url.starts_with(AUTH_ENDPOINT));
        assert!(start.url.contains("code_challenge_method=S256"));
        assert!(start.url.contains("access_type=offline"), "sonst kein Refresh-Token");
        assert!(start.url.contains(&urlencode(SCOPE)));
        assert!(start.url.contains("de.tory.app%3A%2F%2Foauth2"), "{}", start.url);
        // Verifier laenger als die von RFC 7636 verlangten 43 Zeichen.
        assert!(start.code_verifier.len() >= 43);
        assert_eq!(code_challenge(&start.code_verifier).len(), 43);
        assert_ne!(start.code_verifier, code_challenge(&start.code_verifier));
    }

    #[test]
    fn zwei_anmeldeversuche_teilen_kein_geheimnis() {
        let a = begin_auth("id", "de.tory.app://oauth2");
        let b = begin_auth("id", "de.tory.app://oauth2");
        assert_ne!(a.code_verifier, b.code_verifier);
        assert_ne!(a.state, b.state);
    }

    #[test]
    fn schluesselname_haengt_an_der_instanz() {
        assert_eq!(refresh_token_key("privat"), "gmail.privat.refresh_token");
        assert_ne!(refresh_token_key("privat"), refresh_token_key("arbeit"));
    }
}
