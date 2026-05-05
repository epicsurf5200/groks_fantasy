//! Standalone desktop launcher: `ff-gui`. Equivalent to `ff gui`.

use anyhow::{Context, Result};
use clap::Parser;
use groks_fantasy::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "ff-gui", version, about = "Desktop GUI for groks_fantasy")]
struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(short, long)]
    strategy: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let cfg_path = cli.config.unwrap_or_else(config::Config::default_path);
    let mut cfg = config::Config::load(&cfg_path)
        .with_context(|| format!("loading config {}", cfg_path.display()))?;
    if let Some(s) = &cli.strategy {
        cfg.settings.strategy = s.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    }
    let provider: Arc<dyn providers::Provider> = providers::build(&cfg.provider)?.into();
    let anthropic = anthropic::Anthropic::new(cfg.anthropic.clone())?;
    let news_fetcher = Arc::new(news::NewsFetcher::new(cfg.settings.news_sources.clone())?);
    let scheduler = Arc::new(scheduler::Scheduler::new(Duration::from_secs(
        cfg.settings.refresh_seconds,
    )));
    scheduler.spawn(provider.clone(), news_fetcher.clone());
    let rt = tokio::runtime::Handle::current();
    let strategy = cfg.settings.strategy;
    tokio::task::block_in_place(move || {
        gui::run(rt, provider, anthropic, news_fetcher, scheduler, strategy)
    })
}
