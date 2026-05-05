use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use groks_fantasy::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "ff", version, about = "Autonomous AI-powered fantasy football manager")]
struct Cli {
    /// Path to config.yaml (defaults to ./config.yaml or $XDG_CONFIG_HOME/groks_fantasy/config.yaml).
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Override strategy: conservative | balanced | high_stakes.
    #[arg(short, long, global = true)]
    strategy: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Launch the terminal UI (default).
    Ui,
    /// Launch the desktop GUI (requires the `gui` build feature, on by default).
    Gui,
    /// Print the current roster and exit.
    Roster,
    /// Generate an AI-recommended lineup for the current week and exit.
    Lineup {
        /// Override week (defaults to current scoring period).
        #[arg(short, long)]
        week: Option<u8>,
    },
    /// Suggest waiver-wire pickups with metrics + AI re-rank.
    Waiver {
        /// Free-agent pool size to consider (where supported).
        #[arg(short, long, default_value_t = 200)]
        pool: usize,
    },
    /// Analyze a proposed trade.
    Trade {
        /// Partner team name (matched case-insensitively).
        #[arg(short, long)]
        partner: String,
        /// Comma-separated list of players you are sending.
        #[arg(short, long)]
        send: String,
        /// Comma-separated list of players you are receiving.
        #[arg(short, long)]
        receive: String,
    },
    /// Watch the draft and print AI pick suggestions when it's your turn.
    Draft {
        /// Poll interval in seconds.
        #[arg(short = 'i', long, default_value_t = 10)]
        interval: u64,
    },
    /// One-shot: print AI draft suggestions for the next pick and exit.
    DraftSuggest,
    /// Run the background data scheduler in the foreground (debug/cron use).
    Schedule {
        /// Override refresh interval (seconds).
        #[arg(short, long)]
        interval: Option<u64>,
    },
    /// Print the configured league settings and quit.
    Info,
    /// Write a starter config.yaml in the current directory.
    Init {
        /// Provider to scaffold (espn|sleeper|yahoo).
        #[arg(default_value = "sleeper")]
        provider: String,
    },
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

    if let Some(Command::Init { provider }) = &cli.command {
        return write_starter_config(provider);
    }

    let cfg_path = cli
        .config
        .clone()
        .unwrap_or_else(config::Config::default_path);
    let mut cfg = config::Config::load(&cfg_path)
        .with_context(|| format!("loading config {}", cfg_path.display()))?;
    if let Some(s) = &cli.strategy {
        cfg.settings.strategy = s.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    }

    let provider: Arc<dyn providers::Provider> = providers::build(&cfg.provider)?.into();
    let anthropic = anthropic::Anthropic::new(cfg.anthropic.clone())?;
    let news_fetcher = Arc::new(news::NewsFetcher::new(cfg.settings.news_sources.clone())?);

    match cli.command.unwrap_or(Command::Ui) {
        Command::Ui => {
            let app = ui::App::new(
                provider,
                anthropic,
                news_fetcher,
                cfg.settings.strategy,
                Duration::from_secs(cfg.settings.refresh_seconds),
            );
            app.run().await
        }
        Command::Gui => run_gui(provider, anthropic, news_fetcher, cfg).await,
        Command::Roster => print_roster(provider.as_ref()).await,
        Command::Lineup { week } => print_lineup(provider, anthropic, news_fetcher, cfg, week).await,
        Command::Waiver { pool } => print_waiver(provider, anthropic, news_fetcher, cfg, pool).await,
        Command::Trade {
            partner,
            send,
            receive,
        } => print_trade(provider, anthropic, news_fetcher, cfg, partner, send, receive).await,
        Command::Draft { interval } => {
            watch_draft(provider, anthropic, news_fetcher, cfg, interval).await
        }
        Command::DraftSuggest => one_shot_draft(provider, anthropic, news_fetcher, cfg).await,
        Command::Schedule { interval } => {
            run_scheduler_foreground(provider, news_fetcher, cfg, interval).await
        }
        Command::Info => print_info(provider.as_ref()).await,
        Command::Init { .. } => unreachable!(),
    }
}

#[cfg(feature = "gui")]
async fn run_gui(
    provider: Arc<dyn providers::Provider>,
    anthropic: anthropic::Anthropic,
    news_fetcher: Arc<news::NewsFetcher>,
    cfg: config::Config,
) -> Result<()> {
    use scheduler::Scheduler;
    let scheduler = Arc::new(Scheduler::new(Duration::from_secs(
        cfg.settings.refresh_seconds,
    )));
    scheduler.spawn(provider.clone(), news_fetcher.clone());
    let rt = tokio::runtime::Handle::current();
    let strategy = cfg.settings.strategy;
    let provider_for_gui = provider.clone();
    let news_for_gui = news_fetcher.clone();
    let scheduler_for_gui = scheduler.clone();
    // eframe must run on the main thread; spawn_blocking runs it on a worker
    // thread, but Wayland/X11 + glow require the main thread on most platforms.
    // We keep tokio's runtime alive by holding a handle and call the blocking
    // GUI loop directly from within `block_in_place`.
    tokio::task::block_in_place(move || {
        gui::run(
            rt,
            provider_for_gui,
            anthropic,
            news_for_gui,
            scheduler_for_gui,
            strategy,
        )
    })
}

#[cfg(not(feature = "gui"))]
async fn run_gui(
    _provider: Arc<dyn providers::Provider>,
    _anthropic: anthropic::Anthropic,
    _news_fetcher: Arc<news::NewsFetcher>,
    _cfg: config::Config,
) -> Result<()> {
    anyhow::bail!("rebuild with `--features gui` to enable the desktop GUI")
}

async fn print_roster(provider: &dyn providers::Provider) -> Result<()> {
    let roster = provider.my_roster().await?;
    println!(
        "Team: {}  ({}-{}-{}, PF {:.1}, PA {:.1})",
        roster.team_name,
        roster.wins,
        roster.losses,
        roster.ties,
        roster.points_for,
        roster.points_against
    );
    for p in &roster.players {
        println!(
            "  {:<6} {:<24} {:<3} {:<4} {:<4} proj {:>5.1}  avg {:>5.1}",
            p.roster_slot.to_string(),
            p.name,
            p.position.to_string(),
            p.team,
            p.status.to_string(),
            p.projected_points,
            p.avg_points
        );
    }
    Ok(())
}

async fn print_info(provider: &dyn providers::Provider) -> Result<()> {
    let s = provider.league_settings().await?;
    let w = provider.current_week().await?;
    println!("Provider:    {}", provider.name());
    println!("Scoring:     {}", s.scoring);
    println!("Teams:       {}", s.team_count);
    println!("Current week:{w}");
    println!("Roster slots:");
    for (p, c) in &s.roster_slots {
        println!("  {p}: {c}");
    }
    Ok(())
}

async fn print_lineup(
    provider: Arc<dyn providers::Provider>,
    anthropic: anthropic::Anthropic,
    news_fetcher: Arc<news::NewsFetcher>,
    cfg: config::Config,
    week_override: Option<u8>,
) -> Result<()> {
    let settings = provider.league_settings().await.unwrap_or_default();
    let week = match week_override {
        Some(w) => w,
        None => provider.current_week().await.unwrap_or(1),
    };
    let roster = provider.my_roster().await?;
    let matchups = provider.matchups(week).await.unwrap_or_default();
    let mut feed = news_fetcher.fetch_all(50).await;
    let names: Vec<String> = roster.players.iter().map(|p| p.name.clone()).collect();
    let filtered = news::relevant_to(&feed, &names);
    if !filtered.is_empty() {
        feed = filtered;
    }
    let l = lineup::ai_optimize(
        &anthropic,
        &roster,
        &settings,
        &matchups,
        &feed,
        cfg.settings.strategy,
        week,
    )
    .await?;
    println!("Strategy: {}  |  Week: {}", cfg.settings.strategy.label(), week);
    println!("Projected total: {:.1}\n", l.projected_total);
    for s in &l.starters {
        let name = s
            .player
            .as_ref()
            .map(|p| {
                format!(
                    "{} ({} {}) {} proj {:.1}",
                    p.name, p.position, p.team, p.status, p.projected_points
                )
            })
            .unwrap_or_else(|| "(empty)".into());
        println!("  {:<6} {}", s.slot.to_string(), name);
    }
    println!("\nReasoning:\n{}", l.reasoning);
    Ok(())
}

async fn print_waiver(
    provider: Arc<dyn providers::Provider>,
    anthropic: anthropic::Anthropic,
    news_fetcher: Arc<news::NewsFetcher>,
    cfg: config::Config,
    pool: usize,
) -> Result<()> {
    let news = news_fetcher.fetch_all(50).await;
    let report = waiver::analyze(
        provider.as_ref(),
        &anthropic,
        cfg.settings.strategy,
        &news,
        pool,
    )
    .await?;
    println!("Waiver suggestions ({} candidates) — strategy {}",
        report.candidates.len(), cfg.settings.strategy.label());
    for c in &report.candidates {
        let drop = match &c.drop_candidate {
            Some(d) => format!(
                "drop {} (Δ {:+.0} ROS)",
                d.player.name, d.net_ros_delta
            ),
            None => "no drop suggested".into(),
        };
        println!(
            "  {}. {} ({} {})  ROS {:.0}  floor {:.1}  ceil {:.1}  risk {:.2}  fit {:.2}  | {}",
            c.priority,
            c.player.name,
            c.player.position,
            c.player.team,
            c.metrics.ros_value,
            c.metrics.floor,
            c.metrics.ceiling,
            c.metrics.risk_score,
            c.metrics.strategy_fit,
            drop,
        );
    }
    if !report.raw.is_empty() {
        println!("\nClaude analysis:\n{}", report.raw);
    }
    Ok(())
}

async fn print_trade(
    provider: Arc<dyn providers::Provider>,
    anthropic: anthropic::Anthropic,
    news_fetcher: Arc<news::NewsFetcher>,
    cfg: config::Config,
    partner: String,
    send: String,
    receive: String,
) -> Result<()> {
    let my_roster = provider.my_roster().await?;
    let other_rosters = provider.all_rosters().await.unwrap_or_default();
    let news = news_fetcher.fetch_all(50).await;
    let send_v: Vec<String> = send.split(',').map(|s| s.trim().to_string()).collect();
    let recv_v: Vec<String> = receive.split(',').map(|s| s.trim().to_string()).collect();
    let a = trade::analyze(
        &anthropic,
        &my_roster,
        &partner,
        &send_v,
        &recv_v,
        &other_rosters,
        cfg.settings.strategy,
        &news,
    )
    .await?;
    println!("Verdict: {}   net ROS {:+.1}   fairness {:.0}%",
        a.verdict, a.net_ros_delta, a.fairness * 100.0);
    println!("\nYou send (mean {:.1}, ceil {:.1}, ROS {:.0}, risk {:.2}):",
        a.send_pkg.total_mean, a.send_pkg.total_ceiling, a.send_pkg.total_ros, a.send_pkg.avg_risk);
    for m in &a.send {
        println!("  {}", m.one_line());
    }
    println!("\nYou receive (mean {:.1}, ceil {:.1}, ROS {:.0}, risk {:.2}):",
        a.receive_pkg.total_mean, a.receive_pkg.total_ceiling, a.receive_pkg.total_ros, a.receive_pkg.avg_risk);
    for m in &a.receive {
        println!("  {}", m.one_line());
    }
    if !a.ai_summary.is_empty() {
        println!("\nClaude analysis:\n{}", a.ai_summary);
    }
    Ok(())
}

async fn one_shot_draft(
    provider: Arc<dyn providers::Provider>,
    anthropic: anthropic::Anthropic,
    news_fetcher: Arc<news::NewsFetcher>,
    cfg: config::Config,
) -> Result<()> {
    let roster = provider.my_roster().await?;
    let news = news_fetcher.fetch_all(20).await;
    let dm = draft::DraftManager {
        provider: provider.as_ref(),
        anthropic: &anthropic,
        strategy: cfg.settings.strategy,
        my_team_name: roster.team_name.clone(),
        poll_interval: Duration::from_secs(2),
    };
    let state = dm.snapshot().await?;
    let my_picks: Vec<types::DraftPick> = state
        .picks
        .iter()
        .filter(|p| p.team_name.eq_ignore_ascii_case(&roster.team_name))
        .cloned()
        .collect();
    let s = dm.ask_claude(&state, &my_picks, &[], &news).await?;
    println!(
        "Pick #{} (round {} of {}) — {}",
        state.current_pick,
        ((state.current_pick.saturating_sub(1) / state.team_count.max(1)) + 1),
        state.total_rounds,
        cfg.settings.strategy.label()
    );
    for p in &s.picks {
        let pos = p.position.map(|x| x.to_string()).unwrap_or_else(|| "?".into());
        println!("  {}. {} ({}) — {}", p.rank, p.name, pos, p.rationale);
    }
    if s.picks.is_empty() {
        println!("\n--- raw ---\n{}", s.raw);
    }
    Ok(())
}

async fn watch_draft(
    provider: Arc<dyn providers::Provider>,
    anthropic: anthropic::Anthropic,
    news_fetcher: Arc<news::NewsFetcher>,
    cfg: config::Config,
    interval: u64,
) -> Result<()> {
    let roster = provider.my_roster().await?;
    let dm = draft::DraftManager {
        provider: provider.as_ref(),
        anthropic: &anthropic,
        strategy: cfg.settings.strategy,
        my_team_name: roster.team_name.clone(),
        poll_interval: Duration::from_secs(interval),
    };
    let mut last_pick = 0u32;
    println!(
        "Watching draft for team '{}' — strategy {} — every {}s",
        roster.team_name,
        cfg.settings.strategy.label(),
        interval
    );
    loop {
        match dm.snapshot().await {
            Ok(state) => {
                if state.completed {
                    println!("Draft complete.");
                    break;
                }
                if let Some(p) = state.picks.last() {
                    if p.pick_number != last_pick {
                        last_pick = p.pick_number;
                        println!(
                            "  R{}.{} {} → {}",
                            p.round,
                            p.pick_number,
                            p.team_name,
                            p.player_name.as_deref().unwrap_or("?")
                        );
                    }
                }
                let pick_idx = state.current_pick.saturating_sub(1) % state.team_count.max(1);
                let snake_round_even = ((state.current_pick.saturating_sub(1)) / state.team_count.max(1)) % 2 == 1;
                let your_slot_zero_indexed = my_pick_slot(&state, &roster.team_name);
                let on_clock = if snake_round_even {
                    state.team_count.saturating_sub(1).saturating_sub(pick_idx)
                } else {
                    pick_idx
                };
                let is_my_turn = match your_slot_zero_indexed {
                    Some(s) => on_clock == s,
                    None => false,
                };
                if is_my_turn {
                    let news = news_fetcher.fetch_all(20).await;
                    let my_picks: Vec<types::DraftPick> = state
                        .picks
                        .iter()
                        .filter(|p| p.team_name.eq_ignore_ascii_case(&roster.team_name))
                        .cloned()
                        .collect();
                    match dm.ask_claude(&state, &my_picks, &[], &news).await {
                        Ok(s) => {
                            println!(
                                "\n>>> YOU ARE ON THE CLOCK — pick #{} <<<",
                                state.current_pick
                            );
                            for p in &s.picks {
                                let pos =
                                    p.position.map(|x| x.to_string()).unwrap_or_else(|| "?".into());
                                println!("  {}. {} ({}) — {}", p.rank, p.name, pos, p.rationale);
                            }
                            println!();
                        }
                        Err(e) => eprintln!("draft AI error: {e}"),
                    }
                }
            }
            Err(e) => eprintln!("draft snapshot error: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
    Ok(())
}

async fn run_scheduler_foreground(
    provider: Arc<dyn providers::Provider>,
    news_fetcher: Arc<news::NewsFetcher>,
    cfg: config::Config,
    interval_override: Option<u64>,
) -> Result<()> {
    let interval =
        Duration::from_secs(interval_override.unwrap_or(cfg.settings.refresh_seconds));
    println!(
        "Starting background refresh — provider {}, every {}s. Ctrl-C to exit.",
        provider.name(),
        interval.as_secs()
    );
    let scheduler = scheduler::Scheduler::new(interval);
    let handle = scheduler.spawn(provider.clone(), news_fetcher.clone());
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let snap = scheduler.data.read().clone();
        let week = snap.week;
        let roster_size = snap.roster.as_ref().map(|r| r.players.len()).unwrap_or(0);
        let news_size = snap.news.len();
        let last = snap
            .last_refresh
            .map(|t| format!("{}s ago", t.elapsed().as_secs()))
            .unwrap_or_else(|| "never".into());
        let err = snap.last_error.as_deref().unwrap_or("ok");
        println!(
            "[{}] week={week} roster={roster_size} news={news_size} last_refresh={last} status={err}",
            chrono::Local::now().format("%H:%M:%S")
        );
        if handle.is_finished() {
            break;
        }
    }
    Ok(())
}

fn my_pick_slot(state: &types::DraftState, team_name: &str) -> Option<u32> {
    state
        .picks
        .iter()
        .find(|p| p.team_name.eq_ignore_ascii_case(team_name) && p.round == 1)
        .map(|p| p.pick_number.saturating_sub(1))
}

fn write_starter_config(provider: &str) -> Result<()> {
    let path = std::path::Path::new("config.yaml");
    if path.exists() {
        anyhow::bail!("config.yaml already exists; refusing to overwrite");
    }
    let body = match provider {
        "espn" => include_str!("../examples/config.espn.yaml"),
        "yahoo" => include_str!("../examples/config.yahoo.yaml"),
        _ => include_str!("../examples/config.sleeper.yaml"),
    };
    std::fs::write(path, body)?;
    println!("wrote {} ({} starter)", path.display(), provider);
    Ok(())
}
