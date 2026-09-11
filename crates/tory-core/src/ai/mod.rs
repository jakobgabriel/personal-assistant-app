//! Die AI-Schicht — Roadmap-Teil, aber die Rohre liegen.
//!
//! Der Gedanke: Tory hat schon alle Signale des Tages entdoppelt und sortiert.
//! Genau das ist ein guter Prompt. Was fehlt, ist ein Modell, das daraus zwei
//! Saetze macht ("heute zaehlt X, Y kann warten") und spaeter Fragen ueber die
//! eigenen Notizen beantwortet.
//!
//! Drei Dinge sind hier bewusst entschieden:
//!
//! 1. **Der Schluessel liegt im Secret-Store**, nicht in der Konfiguration.
//! 2. **`titles_only` ist Standard an.** Was das Modell sieht, sind Titel,
//!    Zeiten und Dringlichkeiten — keine Mailtexte, keine Notizinhalte. Wer mehr
//!    Qualitaet will, schaltet es aus und weiss, was er teilt.
//! 3. **Drei Anbieter, eine Schnittstelle.** Anthropic und OpenAI ueber ihre
//!    Messages-/Chat-Endpunkte, Ollama fuer "laeuft im eigenen Netz, kein
//!    Schluessel, verlaesst das Haus nicht".
//!
//! Was noch nicht hier ist, steht in `docs/roadmap-ki.md`: Werkzeugaufrufe
//! (Aufgabe anlegen, Notiz suchen), Streaming und ein Frage-Antwort-Feld ueber
//! dem Vault.

pub mod providers;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{AiConfig, AiProviderKind};
use crate::error::{Error, Result};
use crate::model::{Signal, TimeKind};
use crate::secrets::SecretStore;

/// Eine Frage an das Modell. Bewusst schlicht: Systemhaltung plus ein Text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

/// Die Antwort samt Verbrauch — der Verbrauch gehoert in die Oberflaeche, sonst
/// merkt niemand, was ein Tagesbriefing kostet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Completion {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    pub model: String,
}

/// Ein Anbieter. Absichtlich eine Methode — alles Weitere (Streaming,
/// Werkzeuge) kommt erst, wenn es gebraucht wird.
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    async fn complete(&self, prompt: &Prompt, config: &AiConfig) -> Result<Completion>;
}

/// Baut den Anbieter aus der Konfiguration und holt den Schluessel aus dem
/// Secret-Store.
pub fn provider(
    http: reqwest::Client,
    config: &AiConfig,
    secrets: &SecretStore,
) -> Result<Box<dyn Provider>> {
    if !config.enabled {
        return Err(Error::config("AI ist in den Einstellungen ausgeschaltet"));
    }
    let key = match &config.api_key_key {
        Some(name) => Some(secrets.require(name)?.to_string()),
        None if config.provider == AiProviderKind::Ollama => None,
        None => return Err(Error::config("Kein API-Schluessel hinterlegt")),
    };
    Ok(match config.provider {
        AiProviderKind::Anthropic => Box::new(providers::Anthropic::new(http, key.unwrap_or_default())),
        AiProviderKind::OpenAi => Box::new(providers::OpenAi::new(http, key.unwrap_or_default())),
        AiProviderKind::Ollama => Box::new(providers::Ollama::new(
            http,
            config.base_url.clone().unwrap_or_else(|| "http://localhost:11434".into()),
        )),
    })
}

/// Der Schluesselname, unter dem der API-Schluessel liegt.
pub fn api_key_key(provider: AiProviderKind) -> String {
    match provider {
        AiProviderKind::Anthropic => "ai.anthropic.key".into(),
        AiProviderKind::OpenAi => "ai.openai.key".into(),
        AiProviderKind::Ollama => "ai.ollama.key".into(),
    }
}

/// Baut den Prompt fuer das Tagesbriefing aus den Signalen, die der Startscreen
/// ohnehin schon hat.
///
/// `titles_only` entscheidet, ob Auszuege mitgehen. Mit `true` sieht das Modell
/// Titel, Quelle, Zeit und Dringlichkeit — genug, um zu priorisieren, zu wenig,
/// um Inhalte auszuplaudern.
pub fn brief_prompt(signals: &[Signal], now: DateTime<Utc>, titles_only: bool) -> Prompt {
    let system = "Du bist der Assistent in einer persoenlichen Dashboard-App. \
        Du bekommst die Signale des Tages aus verschiedenen Quellen. \
        Antworte auf Deutsch, in hoechstens vier Saetzen, ohne Anrede und ohne Aufzaehlung. \
        Sage zuerst, was heute wirklich zaehlt, dann was warten kann. \
        Erfinde nichts: was nicht in der Liste steht, existiert nicht."
        .to_string();

    let mut user = format!("Jetzt ist {}.\n\nSignale:\n", now.to_rfc3339());
    if signals.is_empty() {
        user.push_str("(keine)\n");
    }
    for signal in signals.iter().take(40) {
        let zeit = match (signal.at, signal.time_kind) {
            (Some(at), Some(TimeKind::Due)) => format!(" faellig {}", at.date_naive()),
            (Some(at), Some(TimeKind::At)) => format!(" um {}", at.format("%d.%m. %H:%M")),
            (Some(at), Some(TimeKind::Window)) => format!(" ab {}", at.format("%d.%m. %H:%M")),
            (Some(at), Some(TimeKind::Since)) => format!(" seit {}", at.format("%d.%m. %H:%M")),
            _ => String::new(),
        };
        user.push_str(&format!(
            "- [{:?}] {} ({}{})",
            signal.urgency,
            signal.title,
            signal.source.label,
            zeit
        ));
        if !titles_only {
            if let Some(excerpt) = &signal.excerpt {
                user.push_str(&format!(" — {excerpt}"));
            }
        }
        user.push('\n');
    }
    Prompt { system, user }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SourceKind, SourceRef, Urgency};
    use chrono::TimeZone;

    fn signal(title: &str, urgency: Urgency, excerpt: Option<&str>) -> Signal {
        Signal {
            urgency,
            excerpt: excerpt.map(str::to_string),
            at: Some(Utc.with_ymd_and_hms(2026, 9, 12, 8, 0, 0).unwrap()),
            time_kind: Some(TimeKind::Due),
            ..Signal::new(SourceRef::new(SourceKind::Mindwtr, "haupt", "Mindwtr"), title, title)
        }
    }

    #[test]
    fn titles_only_haelt_auszuege_zurueck() {
        let signals = vec![signal("Steuer", Urgency::Critical, Some("Belege 2025, IBAN DE12…"))];
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 9, 0, 0).unwrap();

        let knapp = brief_prompt(&signals, now, true);
        assert!(knapp.user.contains("Steuer"));
        assert!(!knapp.user.contains("IBAN"), "Auszug darf nicht mitgehen:\n{}", knapp.user);

        let voll = brief_prompt(&signals, now, false);
        assert!(voll.user.contains("IBAN"));
    }

    #[test]
    fn prompt_nennt_dringlichkeit_quelle_und_frist() {
        let p = brief_prompt(
            &[signal("Dach pruefen", Urgency::High, None)],
            Utc.with_ymd_and_hms(2026, 9, 11, 9, 0, 0).unwrap(),
            true,
        );
        assert!(p.user.contains("[High]"));
        assert!(p.user.contains("Mindwtr"));
        assert!(p.user.contains("faellig 2026-09-12"));
    }

    #[test]
    fn leere_liste_ergibt_trotzdem_einen_prompt() {
        let p = brief_prompt(&[], Utc::now(), true);
        assert!(p.user.contains("(keine)"));
        assert!(!p.system.is_empty());
    }

    #[test]
    fn prompt_ist_begrenzt() {
        let viele: Vec<Signal> =
            (0..100).map(|i| signal(&format!("Aufgabe {i}"), Urgency::Normal, None)).collect();
        let p = brief_prompt(&viele, Utc::now(), true);
        assert_eq!(p.user.lines().filter(|l| l.starts_with("- [")).count(), 40);
    }

    #[test]
    fn ohne_schluessel_kein_anbieter() {
        let dir = std::env::temp_dir().join(format!("tory-ai-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let secrets = SecretStore::open(&dir).unwrap();
        let http = crate::http::client().unwrap();

        let aus = AiConfig::default();
        assert!(provider(http.clone(), &aus, &secrets).is_err(), "ausgeschaltet");

        let mut an = AiConfig { enabled: true, ..AiConfig::default() };
        assert!(provider(http.clone(), &an, &secrets).is_err(), "kein Schluessel");

        // Ollama braucht keinen Schluessel.
        an.provider = AiProviderKind::Ollama;
        assert!(provider(http, &an, &secrets).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
