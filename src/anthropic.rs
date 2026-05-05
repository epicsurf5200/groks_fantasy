use crate::config::AnthropicConfig;
use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

#[derive(Debug, Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    message: String,
    #[serde(default, rename = "type")]
    kind: String,
}

#[derive(Clone)]
pub struct Anthropic {
    http: Client,
    cfg: AnthropicConfig,
}

impl Anthropic {
    pub fn new(cfg: AnthropicConfig) -> Result<Self> {
        if cfg.api_key.trim().is_empty() {
            return Err(anyhow!(
                "missing Anthropic API key (set anthropic.api_key or ANTHROPIC_API_KEY)"
            ));
        }
        let http = Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("groks_fantasy/0.2 (anthropic)")
            .build()?;
        Ok(Self { http, cfg })
    }

    pub fn model(&self) -> &str {
        &self.cfg.model
    }

    pub async fn complete(&self, system: &str, user: &str) -> Result<String> {
        self.complete_with(system, user, None).await
    }

    pub async fn complete_with(
        &self,
        system: &str,
        user: &str,
        temperature: Option<f32>,
    ) -> Result<String> {
        let body = MessagesRequest {
            model: &self.cfg.model,
            max_tokens: self.cfg.max_tokens,
            system,
            messages: vec![Message {
                role: "user",
                content: user,
            }],
            temperature,
        };

        let resp = self
            .http
            .post(API_URL)
            .header("x-api-key", &self.cfg.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("anthropic request failed")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(anyhow!(
                "anthropic API returned {} — {}",
                status,
                truncate(&text, 500)
            ));
        }

        let parsed: MessagesResponse = serde_json::from_str(&text)
            .with_context(|| format!("decoding anthropic response: {}", truncate(&text, 200)))?;

        if let Some(err) = parsed.error {
            return Err(anyhow!("anthropic error [{}]: {}", err.kind, err.message));
        }

        let mut out = String::new();
        for block in parsed.content {
            if let ContentBlock::Text { text } = block {
                out.push_str(&text);
            }
        }
        if out.is_empty() {
            return Err(anyhow!("empty response from Anthropic"));
        }
        Ok(out)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
