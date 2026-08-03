use crate::strategy::Strategy;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnthropicConfig {
    /// API key. Falls back to ANTHROPIC_API_KEY env var when empty.
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_model() -> String {
    "claude-sonnet-4-6".into()
}

fn default_max_tokens() -> u32 {
    2048
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SleeperConfig {
    /// Sleeper username or numeric user_id. Required.
    pub username: String,
    /// Optional — auto-discovered from your leagues when empty.
    #[serde(default)]
    pub league_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub strategy: Strategy,
    #[serde(default = "default_refresh")]
    pub refresh_seconds: u64,
    #[serde(default = "default_news")]
    pub news_sources: Vec<String>,
}

fn default_refresh() -> u64 {
    900
}

fn default_news() -> Vec<String> {
    vec![
        "https://www.espn.com/espn/rss/nfl/news".into(),
        "https://api.sleeper.app/news/nfl/rss".into(),
    ]
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strategy: Strategy::default(),
            refresh_seconds: default_refresh(),
            news_sources: default_news(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub anthropic: AnthropicConfig,
    pub sleeper: SleeperConfig,
    #[serde(default)]
    pub settings: Settings,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config at {}", path.display()))?;
        let mut cfg: Self = serde_yaml::from_str(&raw)
            .with_context(|| format!("parsing config at {}", path.display()))?;
        if cfg.anthropic.api_key.is_empty() {
            if let Ok(env) = std::env::var("ANTHROPIC_API_KEY") {
                cfg.anthropic.api_key = env;
            }
        }
        Ok(cfg)
    }

    pub fn default_path() -> PathBuf {
        if let Ok(custom) = std::env::var("SA_CONFIG") {
            return PathBuf::from(custom);
        }
        let local = PathBuf::from("config.yaml");
        if local.exists() {
            return local;
        }
        if let Some(dir) = dirs::config_dir() {
            return dir.join("sleeper-agent").join("config.yaml");
        }
        local
    }
}
