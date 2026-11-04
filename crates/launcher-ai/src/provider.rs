//! Optional LLM explainer.
//!
//! Two wire formats cover nearly everything people actually run:
//!
//! * [`AiProvider::OpenAi`] — `/chat/completions`, which is what OpenAI, Ollama
//!   (`/v1`), LM Studio, OpenRouter, Groq and most self-hosted gateways speak.
//! * [`AiProvider::Anthropic`] — `/v1/messages`.
//!
//! The API key is never stored here and never logged: it is passed in by the
//! caller, which reads it from the OS keyring at call time.

use serde::Deserialize;

use launcher_core::settings::AiProvider;

use crate::heuristics::Diagnosis;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("no API key is stored for this provider")]
    MissingKey,
    #[error("the endpoint is not a valid URL: {0}")]
    BadEndpoint(String),
    #[error("provider request failed: {0}")]
    Transport(String),
    #[error("provider returned HTTP {status}: {body}")]
    Status { status: u16, body: String },
    #[error("provider response did not contain any text")]
    Empty,
    #[error("provider response could not be parsed: {0}")]
    Parse(String),
}

#[derive(Debug, Clone)]
pub struct AiConfig {
    pub provider: AiProvider,
    /// Base URL without a trailing slash, e.g. `https://api.openai.com/v1`.
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    /// Hard cap for the prompt body. The analyser already trims the log; this
    /// is the second gate before bytes leave the machine.
    pub max_log_bytes: usize,
}

/// Ask the provider to explain a crash. The prompt is built from the redacted
/// log and the offline diagnoses, so the model corrects and expands them instead
/// of starting from zero.
pub async fn explain(
    client: &reqwest::Client,
    config: &AiConfig,
    redacted_log: &str,
    diagnoses: &[Diagnosis],
) -> Result<String, AiError> {
    let key = config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AiError::MissingKey)?;

    let endpoint = config.endpoint.trim_end_matches('/');
    if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
        return Err(AiError::BadEndpoint(endpoint.to_string()));
    }

    let mut log = redacted_log;
    if log.len() > config.max_log_bytes {
        let mut start = log.len() - config.max_log_bytes;
        while start < log.len() && !log.is_char_boundary(start) {
            start += 1;
        }
        log = &log[start..];
    }

    let user = format!(
        "Przyczyna wstępna (analiza offline):\n{}\n\nLog (ocenzurowany):\n```\n{}\n```",
        diagnoses
            .iter()
            .map(|diagnosis| format!("- {} — {}", diagnosis.title, diagnosis.evidence))
            .collect::<Vec<_>>()
            .join("\n"),
        log
    );

    match config.provider {
        AiProvider::OpenAi => {
            let url = format!("{endpoint}/chat/completions");
            let body = serde_json::json!({
                "model": config.model,
                "temperature": 0.2,
                "max_tokens": 700,
                "messages": [
                    { "role": "system", "content": SYSTEM_PROMPT },
                    { "role": "user", "content": user },
                ],
            });
            let response = client
                .post(url)
                .bearer_auth(key)
                .json(&body)
                .send()
                .await
                .map_err(|error| AiError::Transport(error.to_string()))?;
            if !response.status().is_success() {
                return Err(status_error(response).await);
            }
            let parsed: OpenAiResponse = response
                .json()
                .await
                .map_err(|error| AiError::Parse(error.to_string()))?;
            parsed
                .choices
                .into_iter()
                .find_map(|choice| choice.message.content)
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
                .ok_or(AiError::Empty)
        }
        AiProvider::Anthropic => {
            let url = format!("{endpoint}/v1/messages");
            let body = serde_json::json!({
                "model": config.model,
                "max_tokens": 700,
                "system": SYSTEM_PROMPT,
                "messages": [{ "role": "user", "content": user }],
            });
            let response = client
                .post(url)
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await
                .map_err(|error| AiError::Transport(error.to_string()))?;
            if !response.status().is_success() {
                return Err(status_error(response).await);
            }
            let parsed: AnthropicResponse = response
                .json()
                .await
                .map_err(|error| AiError::Parse(error.to_string()))?;
            parsed
                .content
                .into_iter()
                .filter(|block| block.kind == "text")
                .filter_map(|block| block.text)
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
                .into_non_empty()
                .ok_or(AiError::Empty)
        }
    }
}

const SYSTEM_PROMPT: &str = "Jesteś inżynierem wsparcia dla Minecrafta. Dostajesz log crashu i wstępną analizę offline. \
Odpowiedz po polsku, zwięźle, bez wstępu. Struktura: \
1) Najbardziej prawdopodobna przyczyna (jedno zdanie). \
2) Dowód z logu (cytat linii, jeśli istnieje). \
3) Kroki naprawcze — numerowana lista, konkretnie: nazwa moda, folder, ustawienie, wersja. \
Jeśli analiza offline się myli, napisz to wprost. Nie wymyślaj nazw modów, których nie ma w logu. Maksymalnie 200 słów.";

trait IntoNonEmpty {
    fn into_non_empty(self) -> Option<String>;
}

impl IntoNonEmpty for String {
    fn into_non_empty(self) -> Option<String> {
        (!self.is_empty()).then_some(self)
    }
}

async fn status_error(response: reqwest::Response) -> AiError {
    let status = response.status().as_u16();
    // The body can echo the request; truncate before it reaches the UI or logs.
    let body = response.text().await.unwrap_or_default();
    let body = body.chars().take(300).collect::<String>();
    AiError::Status { status, body }
}

#[derive(Deserialize)]
struct OpenAiResponse {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    #[serde(default)]
    content: Vec<AnthropicBlock>,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_key_is_reported_before_any_request() {
        let config = AiConfig {
            provider: AiProvider::OpenAi,
            endpoint: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            api_key: None,
            max_log_bytes: 1024,
        };
        let client = reqwest::Client::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let error = runtime
            .block_on(explain(&client, &config, "log", &[]))
            .expect_err("missing key must fail");
        assert!(matches!(error, AiError::MissingKey));
    }

    #[test]
    fn non_http_endpoint_is_rejected() {
        let config = AiConfig {
            provider: AiProvider::OpenAi,
            endpoint: "file:///etc/passwd".into(),
            model: "m".into(),
            api_key: Some("k".into()),
            max_log_bytes: 1024,
        };
        let client = reqwest::Client::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let error = runtime
            .block_on(explain(&client, &config, "log", &[]))
            .expect_err("non-http endpoint must fail");
        assert!(matches!(error, AiError::BadEndpoint(_)));
    }
}
