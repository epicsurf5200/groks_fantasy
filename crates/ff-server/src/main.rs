//! REST API server for the iOS REST client (option 2).
//!
//! All endpoints require an `Authorization: Bearer <token>` header. The token
//! is read from `FF_SERVER_TOKEN` (env) or the `--token` flag. Pair with
//! `--bind 0.0.0.0:8443` and front it with a TLS reverse proxy (Caddy /
//! nginx) — the iOS App Transport Security policy requires HTTPS for
//! non-localhost hosts.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use clap::Parser;
use groks_fantasy::anthropic::Anthropic;
use groks_fantasy::config::Config;
use groks_fantasy::news::NewsFetcher;
use groks_fantasy::providers::{self, Provider};
use groks_fantasy::scheduler::Scheduler;
use groks_fantasy::strategy::Strategy;
use groks_fantasy::{draft as draft_mod, lineup, trade as trade_mod, types, waiver};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

#[derive(Parser)]
#[command(name = "ff-server", version, about = "REST API for groks_fantasy")]
struct Cli {
    /// Path to config.yaml (same format as the CLI).
    #[arg(short, long, default_value = "config.yaml")]
    config: PathBuf,

    /// Bearer token clients must supply. Falls back to $FF_SERVER_TOKEN.
    #[arg(short, long)]
    token: Option<String>,

    /// Bind address (host:port). Default 127.0.0.1:8088.
    #[arg(short, long, default_value = "127.0.0.1:8088")]
    bind: String,
}

#[derive(Clone)]
struct AppState {
    provider: Arc<dyn Provider>,
    anthropic: Anthropic,
    scheduler: Arc<Scheduler>,
    token: String,
    strategy: Arc<std::sync::RwLock<Strategy>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,tower_http=warn")),
        )
        .init();

    let cli = Cli::parse();
    let token = cli
        .token
        .or_else(|| std::env::var("FF_SERVER_TOKEN").ok())
        .ok_or_else(|| anyhow::anyhow!("provide --token or FF_SERVER_TOKEN"))?;
    if token.len() < 16 {
        anyhow::bail!("bearer token too short — use at least 16 random chars");
    }

    let cfg = Config::load(&cli.config)
        .with_context(|| format!("loading {}", cli.config.display()))?;
    let provider: Arc<dyn Provider> = providers::build(&cfg.provider)?.into();
    let anthropic = Anthropic::new(cfg.anthropic.clone())?;
    let news_fetcher = Arc::new(NewsFetcher::new(cfg.settings.news_sources.clone())?);
    let scheduler = Arc::new(Scheduler::new(Duration::from_secs(
        cfg.settings.refresh_seconds,
    )));
    scheduler.spawn(provider.clone(), news_fetcher.clone());

    let state = AppState {
        provider,
        anthropic,
        scheduler,
        token,
        strategy: Arc::new(std::sync::RwLock::new(cfg.settings.strategy)),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/info", get(info))
        .route("/api/roster", get(roster))
        .route("/api/all-rosters", get(all_rosters))
        .route("/api/news", get(news))
        .route("/api/lineup", post(lineup_endpoint))
        .route("/api/waiver", post(waiver_endpoint))
        .route("/api/trade", post(trade_endpoint))
        .route("/api/draft", get(draft_endpoint))
        .route("/api/draft/suggest", post(draft_suggest_endpoint))
        .route("/api/strategy", post(set_strategy_endpoint))
        .route("/api/refresh", post(refresh_endpoint))
        .layer(CorsLayer::new().allow_origin(Any).allow_headers(Any).allow_methods(Any))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    tracing::info!("listening on http://{}", cli.bind);
    let listener = tokio::net::TcpListener::bind(&cli.bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let h = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let supplied = h.strip_prefix("Bearer ").unwrap_or("").trim();
    if supplied.is_empty() || supplied != state.token {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(serde_json::json!({"error": msg}))).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"ok": true}))
}

#[derive(Serialize)]
struct InfoResp {
    provider: String,
    week: u8,
    strategy: String,
    last_refresh_seconds_ago: Option<u64>,
    last_error: Option<String>,
}

async fn info(State(s): State<AppState>, headers: HeaderMap) -> Result<Json<InfoResp>, AppError> {
    check_auth(&s, &headers)?;
    let snap = s.scheduler.data.read().clone();
    Ok(Json(InfoResp {
        provider: s.provider.name().to_string(),
        week: snap.week,
        strategy: s.strategy.read().unwrap().label().to_string(),
        last_refresh_seconds_ago: snap.last_refresh.map(|t| t.elapsed().as_secs()),
        last_error: snap.last_error,
    }))
}

#[derive(Serialize)]
struct PlayerDto {
    id: String,
    name: String,
    position: String,
    roster_slot: String,
    team: String,
    status: String,
    projected_points: f32,
    avg_points: f32,
}

#[derive(Serialize)]
struct RosterDto {
    team_id: String,
    team_name: String,
    wins: u32,
    losses: u32,
    ties: u32,
    points_for: f32,
    points_against: f32,
    players: Vec<PlayerDto>,
}

fn to_player_dto(p: &types::Player) -> PlayerDto {
    PlayerDto {
        id: p.id.clone(),
        name: p.name.clone(),
        position: p.position.to_string(),
        roster_slot: p.roster_slot.to_string(),
        team: p.team.clone(),
        status: p.status.to_string(),
        projected_points: p.projected_points,
        avg_points: p.avg_points,
    }
}

fn to_roster_dto(r: &types::Roster) -> RosterDto {
    RosterDto {
        team_id: r.team_id.clone(),
        team_name: r.team_name.clone(),
        wins: r.wins,
        losses: r.losses,
        ties: r.ties,
        points_for: r.points_for,
        points_against: r.points_against,
        players: r.players.iter().map(to_player_dto).collect(),
    }
}

async fn roster(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Option<RosterDto>>, AppError> {
    check_auth(&s, &headers)?;
    Ok(Json(s.scheduler.data.read().roster.as_ref().map(to_roster_dto)))
}

async fn all_rosters(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RosterDto>>, AppError> {
    check_auth(&s, &headers)?;
    Ok(Json(
        s.scheduler.data.read().all_rosters.iter().map(to_roster_dto).collect(),
    ))
}

#[derive(Serialize)]
struct NewsDto {
    title: String,
    summary: String,
    source: String,
    url: String,
}

async fn news(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<NewsDto>>, AppError> {
    check_auth(&s, &headers)?;
    Ok(Json(
        s.scheduler
            .data
            .read()
            .news
            .iter()
            .take(60)
            .map(|n| NewsDto {
                title: n.title.clone(),
                summary: n.summary.clone(),
                source: n.source.clone(),
                url: n.url.clone(),
            })
            .collect(),
    ))
}

#[derive(Deserialize, Default)]
struct LineupReq {
    week: Option<u8>,
}

#[derive(Serialize)]
struct LineupSlotDto {
    slot: String,
    player: Option<PlayerDto>,
}

#[derive(Serialize)]
struct LineupDto {
    week: u8,
    starters: Vec<LineupSlotDto>,
    bench: Vec<PlayerDto>,
    projected_total: f32,
    reasoning: String,
}

async fn lineup_endpoint(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LineupReq>,
) -> Result<Json<LineupDto>, AppError> {
    check_auth(&s, &headers)?;
    let strat = *s.strategy.read().unwrap();
    let snap = s.scheduler.data.read().clone();
    let roster = snap
        .roster
        .ok_or_else(|| AppError::BadRequest("no roster yet — call /api/refresh first".into()))?;
    let settings = snap.settings.unwrap_or_default();
    let week = req.week.unwrap_or(snap.week.max(1));
    let l = lineup::ai_optimize(
        &s.anthropic,
        &roster,
        &settings,
        &snap.matchups,
        &snap.news,
        strat,
        week,
    )
    .await
    .map_err(AppError::from)?;
    Ok(Json(LineupDto {
        week: l.week,
        starters: l
            .starters
            .into_iter()
            .map(|sl| LineupSlotDto {
                slot: sl.slot.to_string(),
                player: sl.player.as_ref().map(to_player_dto),
            })
            .collect(),
        bench: l.bench.iter().map(to_player_dto).collect(),
        projected_total: l.projected_total,
        reasoning: l.reasoning,
    }))
}

#[derive(Deserialize, Default)]
struct WaiverReq {
    pool: Option<u32>,
}

#[derive(Serialize)]
struct MetricsDto {
    mean_projection: f32,
    floor: f32,
    ceiling: f32,
    risk_score: f32,
    ros_value: f32,
    strategy_fit: f32,
}

#[derive(Serialize)]
struct WaiverCandDto {
    priority: u32,
    player: PlayerDto,
    metrics: MetricsDto,
    drop_player_name: Option<String>,
    drop_ros_delta: f32,
    reasoning: String,
}

#[derive(Serialize)]
struct WaiverDto {
    candidates: Vec<WaiverCandDto>,
    summary: String,
}

async fn waiver_endpoint(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WaiverReq>,
) -> Result<Json<WaiverDto>, AppError> {
    check_auth(&s, &headers)?;
    let strat = *s.strategy.read().unwrap();
    let news = s.scheduler.data.read().news.clone();
    let r = waiver::analyze(
        s.provider.as_ref(),
        &s.anthropic,
        strat,
        &news,
        req.pool.unwrap_or(200) as usize,
    )
    .await
    .map_err(AppError::from)?;
    Ok(Json(WaiverDto {
        summary: r.raw,
        candidates: r
            .candidates
            .iter()
            .map(|c| WaiverCandDto {
                priority: c.priority,
                player: to_player_dto(&c.player),
                metrics: MetricsDto {
                    mean_projection: c.metrics.mean_projection,
                    floor: c.metrics.floor,
                    ceiling: c.metrics.ceiling,
                    risk_score: c.metrics.risk_score,
                    ros_value: c.metrics.ros_value,
                    strategy_fit: c.metrics.strategy_fit,
                },
                drop_player_name: c.drop_candidate.as_ref().map(|d| d.player.name.clone()),
                drop_ros_delta: c
                    .drop_candidate
                    .as_ref()
                    .map(|d| d.net_ros_delta)
                    .unwrap_or(0.0),
                reasoning: c.reasoning.clone(),
            })
            .collect(),
    }))
}

#[derive(Deserialize)]
struct TradeReq {
    partner: String,
    send: Vec<String>,
    receive: Vec<String>,
}

#[derive(Serialize)]
struct TradeDto {
    verdict: String,
    net_ros_delta: f32,
    fairness: f32,
    send: Vec<MetricsDto>,
    send_names: Vec<String>,
    receive: Vec<MetricsDto>,
    receive_names: Vec<String>,
    send_total_ros: f32,
    receive_total_ros: f32,
    summary: String,
}

async fn trade_endpoint(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<TradeReq>,
) -> Result<Json<TradeDto>, AppError> {
    check_auth(&s, &headers)?;
    let strat = *s.strategy.read().unwrap();
    let snap = s.scheduler.data.read().clone();
    let my_roster = snap
        .roster
        .ok_or_else(|| AppError::BadRequest("no roster yet".into()))?;
    let a = trade_mod::analyze(
        &s.anthropic,
        &my_roster,
        &req.partner,
        &req.send,
        &req.receive,
        &snap.all_rosters,
        strat,
        &snap.news,
    )
    .await
    .map_err(AppError::from)?;
    Ok(Json(TradeDto {
        verdict: a.verdict.to_string(),
        net_ros_delta: a.net_ros_delta,
        fairness: a.fairness,
        send_names: a.send.iter().map(|m| m.player_name.clone()).collect(),
        send: a
            .send
            .iter()
            .map(|m| MetricsDto {
                mean_projection: m.mean_projection,
                floor: m.floor,
                ceiling: m.ceiling,
                risk_score: m.risk_score,
                ros_value: m.ros_value,
                strategy_fit: m.strategy_fit,
            })
            .collect(),
        receive_names: a.receive.iter().map(|m| m.player_name.clone()).collect(),
        receive: a
            .receive
            .iter()
            .map(|m| MetricsDto {
                mean_projection: m.mean_projection,
                floor: m.floor,
                ceiling: m.ceiling,
                risk_score: m.risk_score,
                ros_value: m.ros_value,
                strategy_fit: m.strategy_fit,
            })
            .collect(),
        send_total_ros: a.send_pkg.total_ros,
        receive_total_ros: a.receive_pkg.total_ros,
        summary: a.ai_summary,
    }))
}

#[derive(Serialize)]
struct DraftPickDto {
    pick_number: u32,
    round: u32,
    team_name: String,
    player_name: Option<String>,
    position: Option<String>,
}

#[derive(Serialize)]
struct DraftStateDto {
    picks: Vec<DraftPickDto>,
    current_pick: u32,
    total_rounds: u32,
    team_count: u32,
    completed: bool,
}

async fn draft_endpoint(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Option<DraftStateDto>>, AppError> {
    check_auth(&s, &headers)?;
    Ok(Json(
        s.scheduler.data.read().draft.as_ref().map(|d| DraftStateDto {
            picks: d
                .picks
                .iter()
                .map(|p| DraftPickDto {
                    pick_number: p.pick_number,
                    round: p.round,
                    team_name: p.team_name.clone(),
                    player_name: p.player_name.clone(),
                    position: p.position.map(|x| x.to_string()),
                })
                .collect(),
            current_pick: d.current_pick,
            total_rounds: d.total_rounds,
            team_count: d.team_count,
            completed: d.completed,
        }),
    ))
}

#[derive(Serialize)]
struct DraftSuggestDto {
    rank: u32,
    name: String,
    position: Option<String>,
    rationale: String,
}

async fn draft_suggest_endpoint(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<DraftSuggestDto>>, AppError> {
    check_auth(&s, &headers)?;
    let strat = *s.strategy.read().unwrap();
    let snap = s.scheduler.data.read().clone();
    let team_name = snap.roster.as_ref().map(|r| r.team_name.clone()).unwrap_or_default();
    let news = snap.news.clone();
    let dm = draft_mod::DraftManager {
        provider: s.provider.as_ref(),
        anthropic: &s.anthropic,
        strategy: strat,
        my_team_name: team_name.clone(),
        poll_interval: Duration::from_secs(5),
    };
    let state = dm.snapshot().await.map_err(AppError::from)?;
    let my_picks: Vec<types::DraftPick> = state
        .picks
        .iter()
        .filter(|p| p.team_name.eq_ignore_ascii_case(&team_name))
        .cloned()
        .collect();
    let sg = dm
        .ask_claude(&state, &my_picks, &[], &news)
        .await
        .map_err(AppError::from)?;
    Ok(Json(
        sg.picks
            .into_iter()
            .map(|p| DraftSuggestDto {
                rank: p.rank,
                name: p.name,
                position: p.position.map(|x| x.to_string()),
                rationale: p.rationale,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct StrategyReq {
    kind: String,
}

async fn set_strategy_endpoint(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<StrategyReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    check_auth(&s, &headers)?;
    let parsed: Strategy = req.kind.parse().map_err(|e: String| AppError::BadRequest(e))?;
    *s.strategy.write().unwrap() = parsed;
    Ok(Json(
        serde_json::json!({"strategy": parsed.label().to_string()}),
    ))
}

async fn refresh_endpoint(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    check_auth(&s, &headers)?;
    s.scheduler.poke();
    Ok(Json(serde_json::json!({"ok": true})))
}
