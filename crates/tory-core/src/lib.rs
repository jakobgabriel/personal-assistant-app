//! Tory — Kern.
//!
//! Diese Kiste enthaelt das Datenmodell, die Konfiguration, die Anbindungen und
//! den Sync — und **keine** Abhaengigkeit zu Tauri oder einer Oberflaeche. Das
//! ist Absicht: so laesst sich alles Wesentliche ohne Android-SDK, ohne WebView
//! und ohne Netz testen (`cargo test -p tory-core`).
//!
//! Die Tauri-App in `app/src-tauri` ist nur eine Schale: sie reicht Aufrufe der
//! Oberflaeche an [`engine::Engine`] weiter.

pub mod ai;
pub mod config;
pub mod connectors;
pub mod engine;
pub mod error;
pub mod http;
pub mod markdown;
pub mod model;
pub mod secrets;
pub mod store;
pub mod sync;

pub use error::{Error, Result};
