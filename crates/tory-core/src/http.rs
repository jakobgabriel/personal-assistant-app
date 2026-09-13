//! Ein gemeinsamer HTTP-Client und die Uebersetzung von Statuscodes in
//! [`SyncFault`]. Jede Quelle bekaeme sonst ihre eigene, leicht andere
//! Fehlerbehandlung — und der Nutzer drei Formulierungen fuer "Token abgelaufen".

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use reqwest::{Client, Response, StatusCode};

use crate::error::{Error, Result};
use crate::model::SyncFault;

/// Selbst gehostete Dienste haengen manchmal; 20 s ist die Grenze, ab der ein
/// Sync im Hintergrund eher stoert als hilft.
const TIMEOUT: Duration = Duration::from_secs(20);

pub fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(TIMEOUT)
        .connect_timeout(Duration::from_secs(8))
        .user_agent(concat!("Tory/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// Prueft den Status und macht aus einem Fehler die Variante, die die
/// Oberflaeche unterschiedlich darstellt.
pub async fn expect_ok(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .map(|secs| Utc::now() + ChronoDuration::seconds(secs));
    let body = response.text().await.unwrap_or_default();
    let detail = kurzfassung(&body);

    Err(Error::Sync(match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            SyncFault::AuthExpired { detail: if detail.is_empty() { status.to_string() } else { detail } }
        }
        StatusCode::NOT_FOUND => SyncFault::Misconfigured {
            detail: format!("Nicht gefunden (404). Stimmen URL und Kennungen? {detail}"),
        },
        StatusCode::TOO_MANY_REQUESTS => SyncFault::RateLimited { retry_after },
        s if s.is_server_error() => SyncFault::Server { status: s.as_u16(), detail },
        s => SyncFault::Server { status: s.as_u16(), detail },
    }))
}

/// Macht aus einem Antwortkoerper eine Zeile, die in ein Banner passt.
///
/// Server antworten auf einen Fehler gern mit einer ganzen HTML-Seite. Die roh
/// in die Oberflaeche zu kippen, war genau das: ein Banner voller
/// `<!DOCTYPE HTML PUBLIC …>`, in dem die eigentliche Aussage untergeht. Aus
/// HTML bleibt deshalb nur der Titel, und auch der gekuerzt.
fn kurzfassung(body: &str) -> String {
    let trimmed = body.trim();
    let sieht_nach_html_aus = trimmed.starts_with('<')
        || trimmed.get(..20).is_some_and(|a| a.to_ascii_lowercase().contains("<html"));

    if sieht_nach_html_aus {
        let lower = trimmed.to_ascii_lowercase();
        if let Some(start) = lower.find("<title>") {
            let rest = &trimmed[start + "<title>".len()..];
            if let Some(end) = rest.to_ascii_lowercase().find("</title>") {
                let titel = rest[..end].trim();
                if !titel.is_empty() {
                    return format!("Server meldet \u{201e}{}\u{201c}", kuerzen(titel, 80));
                }
            }
        }
        return "Server antwortete mit einer HTML-Seite statt mit Daten".to_string();
    }
    kuerzen(trimmed, 240)
}

fn kuerzen(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max {
        return text;
    }
    format!("{}\u{2026}", text.chars().take(max).collect::<String>())
}

/// Haengt einen Pfad an eine Basis-URL, ohne doppelte oder fehlende Schraegstriche.
pub fn join(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::join;

    use super::kurzfassung;

    #[test]
    fn html_fehlerseiten_landen_nicht_im_banner() {
        let apache = r#"<!DOCTYPE HTML PUBLIC "-//IETF//DTD HTML 2.0//EN">
<html><head> <title>404 Not Found</title></head><body>
<h1>Not Found</h1> <p>The requested URL was not found on this server.</p>
</body></html>"#;
        let kurz = kurzfassung(apache);
        assert!(!kurz.contains('<'), "kein Markup mehr: {kurz}");
        assert!(kurz.contains("404 Not Found"), "der Titel traegt die Aussage: {kurz}");
        assert!(kurz.chars().count() < 60);
    }

    #[test]
    fn html_ohne_titel_wird_trotzdem_lesbar() {
        let kurz = kurzfassung("<html><body><h1>Nope</h1></body></html>");
        assert_eq!(kurz, "Server antwortete mit einer HTML-Seite statt mit Daten");
    }

    #[test]
    fn json_fehler_bleiben_erhalten() {
        let kurz = kurzfassung("{\"error\":\"table not found\"}");
        assert!(kurz.contains("table not found"), "{kurz}");
    }

    #[test]
    fn sehr_lange_koerper_werden_gekuerzt() {
        let kurz = kurzfassung(&"x ".repeat(500));
        assert!(kurz.chars().count() <= 241, "{}", kurz.chars().count());
    }

    #[test]
    fn join_normalisiert_schraegstriche() {
        assert_eq!(join("https://a.de", "v1/tasks"), "https://a.de/v1/tasks");
        assert_eq!(join("https://a.de/", "/v1/tasks"), "https://a.de/v1/tasks");
        assert_eq!(join("https://a.de/sub/", "v1"), "https://a.de/sub/v1");
    }
}
