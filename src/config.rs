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
pub struct EspnConfig {
    pub league_id: u64,
    pub season: u32,
    #[serde(default)]
    pub team_id: Option<u32>,
    /// Required for private leagues.
    #[serde(default)]
    pub swid: String,
    #[serde(default)]
    pub espn_s2: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SleeperConfig {
    pub league_id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub draft_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct YahooConfig {
    pub league_key: String,
    #[serde(default)]
    pub team_key: Option<String>,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ProviderConfig {
    Espn(EspnConfig),
    Sleeper(SleeperConfig),
    Yahoo(YahooConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub strategy: Strategy,
    #[serde(default = "default_scoring")]
    pub scoring_type: String,
    #[serde(default = "default_refresh")]
    pub refresh_seconds: u64,
    #[serde(default)]
    pub auto_set_lineup: bool,
    #[serde(default)]
    pub news_sources: Vec<String>,
}

fn default_scoring() -> String {
    "ppr".into()
}

fn default_refresh() -> u64 {
    900
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strategy: Strategy::default(),
            scoring_type: default_scoring(),
            refresh_seconds: default_refresh(),
            auto_set_lineup: false,
            news_sources: vec![
                "https://www.espn.com/espn/rss/nfl/news".into(),
                "https://api.sleeper.app/news/nfl/rss".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub anthropic: AnthropicConfig,
    pub provider: ProviderConfig,
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
        if let Ok(custom) = std::env::var("FF_CONFIG") {
            return PathBuf::from(custom);
        }
        let local = PathBuf::from("config.yaml");
        if local.exists() {
            return local;
        }
        if let Some(home) = dirs::config_dir() {
            return home.join("groks_fantasy").join("config.yaml");
        }
        local
    }
}
