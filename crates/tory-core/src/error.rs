//! Ein Fehlertyp fuer die ganze Kiste. Die Uebersetzung in das, was die
//! Oberflaeche anzeigt, macht [`crate::model::SyncFault`].

use crate::model::SyncFault;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Dateisystem: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Datenbank: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("Netz: {0}")]
    Http(#[from] reqwest::Error),

    #[error("XML: {0}")]
    Xml(String),

    #[error("Geheimnisse: {0}")]
    Secrets(String),

    #[error("Konfiguration: {0}")]
    Config(String),

    #[error("{}", .0.message())]
    Sync(SyncFault),

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }

    pub fn config(msg: impl Into<String>) -> Self {
        Error::Config(msg.into())
    }

    /// Wie der Nutzer den Fehler auf dem Startscreen sieht.
    pub fn as_fault(&self) -> SyncFault {
        match self {
            Error::Sync(f) => f.clone(),
            Error::Http(e) if e.is_connect() || e.is_timeout() => SyncFault::Offline,
            Error::Http(e) => SyncFault::Unknown { detail: e.to_string() },
            Error::Secrets(d) => SyncFault::Misconfigured { detail: d.clone() },
            Error::Config(d) => SyncFault::Misconfigured { detail: d.clone() },
            other => SyncFault::Unknown { detail: other.to_string() },
        }
    }
}

impl From<SyncFault> for Error {
    fn from(f: SyncFault) -> Self {
        Error::Sync(f)
    }
}
