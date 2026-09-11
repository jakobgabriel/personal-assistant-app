//! Die drei Anbieter. Jeder ist duenn: Anfrage bauen, Antwort lesen, Verbrauch
//! mitnehmen. Die Aufbereitung des Prompts steckt in [`super::brief_prompt`] und
//! ist damit anbieterunabhaengig testbar.

use serde::Deserialize;
use serde_json::json;

use crate::config::AiConfig;
use crate::error::{Error, Result};
use crate::http::expect_ok;

use super::{Completion, Prompt, Provider};

/// Claude ueber die Messages-API.
pub struct Anthropic {
    http: reqwest::Client,
    key: String,
}

impl Anthropic {
    pub fn new(http: reqwest::Client, key: String) -> Self {
        Self { http, key }
    }
}

#[derive(Deserialize)]
struct AnthropicResponse {
    #[serde(default)]
    content: Vec<AnthropicBlock>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: Option<u32>,
    #[serde(default)]
    output_tokens: Option<u32>,
}

#[async_trait::async_trait]
impl Provider for Anthropic {
    async fn complete(&self, prompt: &Prompt, config: &AiConfig) -> Result<Completion> {
        let base = config.base_url.as_deref().unwrap_or("https://api.anthropic.com");
        let body = json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "system": prompt.system,
            "messages": [{ "role": "user", "content": prompt.user }],
        });
        let response: AnthropicResponse = expect_ok(
            self.http
                .post(crate::http::join(base, "v1/messages"))
                .header("x-api-key", &self.key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await?,
        )
        .await?
        .json()
        .await?;

        Ok(Completion {
            text: response.content.iter().filter_map(|b| b.text.clone()).collect::<Vec<_>>().join(""),
            input_tokens: response.usage.as_ref().and_then(|u| u.input_tokens),
            output_tokens: response.usage.as_ref().and_then(|u| u.output_tokens),
            model: response.model.unwrap_or_else(|| config.model.clone()),
        })
    }
}

/// OpenAI und alles, was dessen Chat-Completions-Form spricht.
pub struct OpenAi {
    http: reqwest::Client,
    key: String,
}

impl OpenAi {
    pub fn new(http: reqwest::Client, key: String) -> Self {
        Self { http, key }
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: Option<ChatMessage>,
}

#[derive(Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
}

#[async_trait::async_trait]
impl Provider for OpenAi {
    async fn complete(&self, prompt: &Prompt, config: &AiConfig) -> Result<Completion> {
        let base = config.base_url.as_deref().unwrap_or("https://api.openai.com");
        let body = json!({
            "model": config.model,
            "max_completion_tokens": config.max_tokens,
            "messages": [
                { "role": "system", "content": prompt.system },
                { "role": "user", "content": prompt.user },
            ],
        });
        let response: ChatResponse = expect_ok(
            self.http
                .post(crate::http::join(base, "v1/chat/completions"))
                .bearer_auth(&self.key)
                .json(&body)
                .send()
                .await?,
        )
        .await?
        .json()
        .await?;

        Ok(Completion {
            text: first_message(&response.choices)?,
            input_tokens: response.usage.as_ref().and_then(|u| u.prompt_tokens),
            output_tokens: response.usage.as_ref().and_then(|u| u.completion_tokens),
            model: response.model.unwrap_or_else(|| config.model.clone()),
        })
    }
}

/// Ollama im eigenen Netz. Kein Schluessel, und die Daten verlassen das Haus nicht.
pub struct Ollama {
    http: reqwest::Client,
    base_url: String,
}

impl Ollama {
    pub fn new(http: reqwest::Client, base_url: String) -> Self {
        Self { http, base_url }
    }
}

#[derive(Deserialize)]
struct OllamaResponse {
    #[serde(default)]
    message: Option<ChatMessage>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
    #[serde(default)]
    model: Option<String>,
}

#[async_trait::async_trait]
impl Provider for Ollama {
    async fn complete(&self, prompt: &Prompt, config: &AiConfig) -> Result<Completion> {
        let body = json!({
            "model": config.model,
            "stream": false,
            "options": { "num_predict": config.max_tokens },
            "messages": [
                { "role": "system", "content": prompt.system },
                { "role": "user", "content": prompt.user },
            ],
        });
        let response: OllamaResponse = expect_ok(
            self.http.post(crate::http::join(&self.base_url, "api/chat")).json(&body).send().await?,
        )
        .await?
        .json()
        .await?;

        Ok(Completion {
            text: response
                .message
                .and_then(|m| m.content)
                .ok_or_else(|| Error::other("Ollama hat keinen Text geliefert"))?,
            input_tokens: response.prompt_eval_count,
            output_tokens: response.eval_count,
            model: response.model.unwrap_or_else(|| config.model.clone()),
        })
    }
}

fn first_message(choices: &[ChatChoice]) -> Result<String> {
    choices
        .first()
        .and_then(|c| c.message.as_ref())
        .and_then(|m| m.content.clone())
        .ok_or_else(|| Error::other("Antwort enthielt keinen Text"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_antwort_wird_gelesen() {
        let raw = r#"{
            "id": "msg_1", "model": "claude-sonnet-5", "role": "assistant",
            "content": [{"type": "text", "text": "Heute zaehlt die Steuer."}],
            "usage": {"input_tokens": 412, "output_tokens": 28}
        }"#;
        let parsed: AnthropicResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.content[0].text.as_deref(), Some("Heute zaehlt die Steuer."));
        assert_eq!(parsed.usage.unwrap().input_tokens, Some(412));
    }

    #[test]
    fn anthropic_mehrere_bloecke_werden_zusammengefuegt() {
        let raw = r#"{"content":[{"text":"Teil eins. "},{"text":"Teil zwei."}]}"#;
        let parsed: AnthropicResponse = serde_json::from_str(raw).unwrap();
        let text: String = parsed.content.iter().filter_map(|b| b.text.clone()).collect();
        assert_eq!(text, "Teil eins. Teil zwei.");
    }

    #[test]
    fn openai_antwort_wird_gelesen() {
        let raw = r#"{
            "model": "gpt-4o-mini",
            "choices": [{"index":0,"message":{"role":"assistant","content":"Zwei Saetze."}}],
            "usage": {"prompt_tokens": 300, "completion_tokens": 20}
        }"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(first_message(&parsed.choices).unwrap(), "Zwei Saetze.");
    }

    #[test]
    fn leere_antwort_ist_ein_fehler_kein_leerer_text() {
        let parsed: ChatResponse = serde_json::from_str(r#"{"choices":[]}"#).unwrap();
        assert!(first_message(&parsed.choices).is_err());
    }

    #[test]
    fn ollama_antwort_wird_gelesen() {
        let raw = r#"{
            "model": "llama3.2", "created_at": "2026-09-11T07:00:00Z",
            "message": {"role": "assistant", "content": "Alles ruhig."},
            "done": true, "prompt_eval_count": 120, "eval_count": 15
        }"#;
        let parsed: OllamaResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.message.unwrap().content.as_deref(), Some("Alles ruhig."));
        assert_eq!(parsed.eval_count, Some(15));
    }
}
