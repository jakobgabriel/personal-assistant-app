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
    // Fehlertexte sind oft lang und enthalten Antwortkoerper; gekuerzt reicht.
    let body = response.text().await.unwrap_or_default();
    let detail = body.chars().take(240).collect::<String>();

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

/// Haengt einen Pfad an eine Basis-URL, ohne doppelte oder fehlende Schraegstriche.
pub fn join(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::join;

    #[test]
    fn join_normalisiert_schraegstriche() {
        assert_eq!(join("https://a.de", "v1/tasks"), "https://a.de/v1/tasks");
        assert_eq!(join("https://a.de/", "/v1/tasks"), "https://a.de/v1/tasks");
        assert_eq!(join("https://a.de/sub/", "v1"), "https://a.de/sub/v1");
    }
}
