//! UniFFI surface for the `groks_fantasy` core. Exposes a single `FfClient`
//! object whose methods are synchronous from the FFI's POV — internally we
//! drive the async core via a Tokio runtime so the Swift side just sees
//! ordinary throwing functions and can wrap them in `Task.detached { ... }`.

#![allow(clippy::too_many_arguments)]

use std::sync::Arc;
use std::time::Duration;

use groks_fantasy::anthropic::Anthropic;
use groks_fantasy::config::Config;
use groks_fantasy::news::NewsFetcher;
use groks_fantasy::providers::{self, Provider};
use groks_fantasy::scheduler::Scheduler;
use groks_fantasy::strategy::Strategy;
use groks_fantasy::{draft as draft_mod, lineup, trade as trade_mod, types, waiver};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfError {
    #[error("config: {0}")]
    Config(String),
    #[error("init: {0}")]
    Init(String),
    #[error("api: {0}")]
    Api(String),
    #[error("invalid input: {0}")]
    Input(String),
}

impl From<anyhow::Error> for FfError {
    fn from(e: anyhow::Error) -> Self {
        FfError::Api(e.to_string())
    }
}

// -- DTOs ---------------------------------------------------------------------

#[derive(uniffi::Record, Clone)]
pub struct UPlayer {
    pub id: String,
    pub name: String,
    pub position: String,
    pub roster_slot: String,
    pub team: String,
    pub status: String,
    pub projected_points: f32,
    pub avg_points: f32,
}

#[derive(uniffi::Record, Clone)]
pub struct URoster {
    pub team_id: String,
    pub team_name: String,
    pub wins: u32,
    pub losses: u32,
    pub ties: u32,
    pub points_for: f32,
    pub points_against: f32,
    pub players: Vec<UPlayer>,
}

#[derive(uniffi::Record, Clone)]
pub struct ULineupSlot {
    pub slot: String,
    pub player: Option<UPlayer>,
}

#[derive(uniffi::Record, Clone)]
pub struct ULineup {
    pub week: u8,
    pub starters: Vec<ULineupSlot>,
    pub bench: Vec<UPlayer>,
    pub projected_total: f32,
    pub reasoning: String,
}

#[derive(uniffi::Record, Clone)]
pub struct UMetrics {
    pub mean_projection: f32,
    pub floor: f32,
    pub ceiling: f32,
    pub variance: f32,
    pub risk_score: f32,
    pub trend: f32,
    pub injury_adj: f32,
    pub adjusted_next_week: f32,
    pub ros_value: f32,
    pub strategy_fit: f32,
}

#[derive(uniffi::Record, Clone)]
pub struct UWaiverCandidate {
    pub priority: u32,
    pub player: UPlayer,
    pub metrics: UMetrics,
    pub drop_player_name: Option<String>,
    pub drop_ros_delta: f32,
    pub reasoning: String,
}

#[derive(uniffi::Record, Clone)]
pub struct UWaiverReport {
    pub candidates: Vec<UWaiverCandidate>,
    pub summary: String,
}

#[derive(uniffi::Record, Clone)]
pub struct UTradeAnalysis {
    pub verdict: String,
    pub net_ros_delta: f32,
    pub fairness: f32,
    pub send: Vec<UMetrics>,
    pub send_names: Vec<String>,
    pub receive: Vec<UMetrics>,
    pub receive_names: Vec<String>,
    pub send_total_ros: f32,
    pub receive_total_ros: f32,
    pub send_total_ceiling: f32,
    pub receive_total_ceiling: f32,
    pub summary: String,
}

#[derive(uniffi::Record, Clone)]
pub struct UDraftPick {
    pub pick_number: u32,
    pub round: u32,
    pub team_name: String,
    pub player_name: Option<String>,
    pub position: Option<String>,
}

#[derive(uniffi::Record, Clone)]
pub struct UDraftState {
    pub picks: Vec<UDraftPick>,
    pub current_pick: u32,
    pub total_rounds: u32,
    pub team_count: u32,
    pub completed: bool,
}

#[derive(uniffi::Record, Clone)]
pub struct UDraftSuggestion {
    pub rank: u32,
    pub name: String,
    pub position: Option<String>,
    pub rationale: String,
}

#[derive(uniffi::Record, Clone)]
pub struct UNewsItem {
    pub title: String,
    pub summary: String,
    pub source: String,
    pub url: String,
}

// -- Conversions --------------------------------------------------------------

fn to_uplayer(p: &types::Player) -> UPlayer {
    UPlayer {
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

fn to_uroster(r: &types::Roster) -> URoster {
    URoster {
        team_id: r.team_id.clone(),
        team_name: r.team_name.clone(),
        wins: r.wins,
        losses: r.losses,
        ties: r.ties,
        points_for: r.points_for,
        points_against: r.points_against,
        players: r.players.iter().map(to_uplayer).collect(),
    }
}

fn to_umetrics(m: &groks_fantasy::metrics::PlayerMetrics) -> UMetrics {
    UMetrics {
        mean_projection: m.mean_projection,
        floor: m.floor,
        ceiling: m.ceiling,
        variance: m.variance,
        risk_score: m.risk_score,
        trend: m.trend,
        injury_adj: m.injury_adj,
        adjusted_next_week: m.adjusted_next_week,
        ros_value: m.ros_value,
        strategy_fit: m.strategy_fit,
    }
}

// -- Client -------------------------------------------------------------------

#[derive(uniffi::Object)]
pub struct FfClient {
    rt: tokio::runtime::Runtime,
    provider: Arc<dyn Provider>,
    anthropic: Anthropic,
    #[allow(dead_code)]
    news_fetcher: Arc<NewsFetcher>,
    scheduler: Arc<Scheduler>,
    strategy: std::sync::Mutex<Strategy>,
}

#[uniffi::export]
impl FfClient {
    /// Initialize from an in-memory YAML config string. Pass the contents of
    /// your `config.yaml` (the iOS app keeps it in the App Group container
    /// or asks the user to paste it on first run).
    #[uniffi::constructor]
    pub fn new(config_yaml: String, anthropic_api_key_override: Option<String>) -> Result<Arc<Self>, FfError> {
        let mut cfg: Config = serde_yaml::from_str(&config_yaml)
            .map_err(|e| FfError::Config(e.to_string()))?;
        if let Some(k) = anthropic_api_key_override.filter(|s| !s.is_empty()) {
            cfg.anthropic.api_key = k;
        }
        let provider: Arc<dyn Provider> = providers::build(&cfg.provider)
            .map_err(|e| FfError::Init(e.to_string()))?
            .into();
        let anthropic = Anthropic::new(cfg.anthropic.clone())
            .map_err(|e| FfError::Init(e.to_string()))?;
        let news_fetcher = Arc::new(
            NewsFetcher::new(cfg.settings.news_sources.clone())
                .map_err(|e| FfError::Init(e.to_string()))?,
        );
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .map_err(|e| FfError::Init(e.to_string()))?;
        let scheduler = Arc::new(Scheduler::new(Duration::from_secs(
            cfg.settings.refresh_seconds,
        )));
        scheduler.spawn(provider.clone(), news_fetcher.clone());
        let _enter = rt.enter();
        Ok(Arc::new(Self {
            rt,
            provider,
            anthropic,
            news_fetcher,
            scheduler,
            strategy: std::sync::Mutex::new(cfg.settings.strategy),
        }))
    }

    /// Override the active strategy. `kind` is one of: conservative, balanced, high_stakes.
    pub fn set_strategy(&self, kind: String) -> Result<(), FfError> {
        let s: Strategy = kind.parse().map_err(|e: String| FfError::Input(e))?;
        *self.strategy.lock().unwrap() = s;
        Ok(())
    }

    pub fn current_strategy(&self) -> String {
        self.strategy.lock().unwrap().label().to_string()
    }

    /// Force the background scheduler to refresh now.
    pub fn refresh_now(&self) {
        self.scheduler.poke();
    }

    pub fn current_week(&self) -> u8 {
        self.scheduler.data.read().week
    }

    pub fn last_refresh_seconds_ago(&self) -> Option<u64> {
        self.scheduler.data.read().last_refresh.map(|t| t.elapsed().as_secs())
    }

    pub fn last_error(&self) -> Option<String> {
        self.scheduler.data.read().last_error.clone()
    }

    pub fn roster(&self) -> Option<URoster> {
        self.scheduler.data.read().roster.as_ref().map(to_uroster)
    }

    pub fn all_rosters(&self) -> Vec<URoster> {
        self.scheduler
            .data
            .read()
            .all_rosters
            .iter()
            .map(to_uroster)
            .collect()
    }

    pub fn news(&self, limit: u32) -> Vec<UNewsItem> {
        self.scheduler
            .data
            .read()
            .news
            .iter()
            .take(limit as usize)
            .map(|n| UNewsItem {
                title: n.title.clone(),
                summary: n.summary.clone(),
                source: n.source.clone(),
                url: n.url.clone(),
            })
            .collect()
    }

    /// Synchronously generate an AI lineup. Blocks the caller; wrap in
    /// `Task.detached { try await ... }` from Swift.
    pub fn lineup(&self, week_override: Option<u8>) -> Result<ULineup, FfError> {
        let strat = *self.strategy.lock().unwrap();
        let snap = self.scheduler.data.read().clone();
        let roster = snap
            .roster
            .ok_or_else(|| FfError::Api("no roster yet — call refresh_now() and wait".into()))?;
        let settings = snap.settings.unwrap_or_default();
        let week = week_override.unwrap_or(snap.week.max(1));
        let anthropic = self.anthropic.clone();
        let news = snap.news.clone();
        let matchups = snap.matchups.clone();
        let l = self
            .rt
            .block_on(async move {
                lineup::ai_optimize(&anthropic, &roster, &settings, &matchups, &news, strat, week)
                    .await
            })
            .map_err(FfError::from)?;
        Ok(ULineup {
            week: l.week,
            starters: l
                .starters
                .iter()
                .map(|s| ULineupSlot {
                    slot: s.slot.to_string(),
                    player: s.player.as_ref().map(to_uplayer),
                })
                .collect(),
            bench: l.bench.iter().map(to_uplayer).collect(),
            projected_total: l.projected_total,
            reasoning: l.reasoning,
        })
    }

    pub fn waivers(&self, pool: u32) -> Result<UWaiverReport, FfError> {
        let strat = *self.strategy.lock().unwrap();
        let news = self.scheduler.data.read().news.clone();
        let provider = self.provider.clone();
        let anthropic = self.anthropic.clone();
        let r = self
            .rt
            .block_on(async move {
                waiver::analyze(provider.as_ref(), &anthropic, strat, &news, pool as usize).await
            })
            .map_err(FfError::from)?;
        Ok(UWaiverReport {
            candidates: r
                .candidates
                .iter()
                .map(|c| UWaiverCandidate {
                    priority: c.priority,
                    player: to_uplayer(&c.player),
                    metrics: to_umetrics(&c.metrics),
                    drop_player_name: c.drop_candidate.as_ref().map(|d| d.player.name.clone()),
                    drop_ros_delta: c
                        .drop_candidate
                        .as_ref()
                        .map(|d| d.net_ros_delta)
                        .unwrap_or(0.0),
                    reasoning: c.reasoning.clone(),
                })
                .collect(),
            summary: r.raw,
        })
    }

    pub fn analyze_trade(
        &self,
        partner: String,
        send_csv: String,
        receive_csv: String,
    ) -> Result<UTradeAnalysis, FfError> {
        let strat = *self.strategy.lock().unwrap();
        let snap = self.scheduler.data.read().clone();
        let my_roster = snap
            .roster
            .ok_or_else(|| FfError::Api("no roster yet".into()))?;
        let other_rosters = snap.all_rosters.clone();
        let news = snap.news.clone();
        let send: Vec<String> = send_csv
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let recv: Vec<String> = receive_csv
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if send.is_empty() || recv.is_empty() || partner.trim().is_empty() {
            return Err(FfError::Input(
                "partner / send / receive must all be non-empty".into(),
            ));
        }
        let anthropic = self.anthropic.clone();
        let a = self
            .rt
            .block_on(async move {
                trade_mod::analyze(
                    &anthropic,
                    &my_roster,
                    &partner,
                    &send,
                    &recv,
                    &other_rosters,
                    strat,
                    &news,
                )
                .await
            })
            .map_err(FfError::from)?;
        Ok(UTradeAnalysis {
            verdict: a.verdict.to_string(),
            net_ros_delta: a.net_ros_delta,
            fairness: a.fairness,
            send_names: a.send.iter().map(|m| m.player_name.clone()).collect(),
            send: a.send.iter().map(to_umetrics).collect(),
            receive_names: a.receive.iter().map(|m| m.player_name.clone()).collect(),
            receive: a.receive.iter().map(to_umetrics).collect(),
            send_total_ros: a.send_pkg.total_ros,
            receive_total_ros: a.receive_pkg.total_ros,
            send_total_ceiling: a.send_pkg.total_ceiling,
            receive_total_ceiling: a.receive_pkg.total_ceiling,
            summary: a.ai_summary,
        })
    }

    pub fn draft_state(&self) -> Option<UDraftState> {
        self.scheduler.data.read().draft.as_ref().map(|d| UDraftState {
            picks: d
                .picks
                .iter()
                .map(|p| UDraftPick {
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
        })
    }

    pub fn draft_suggestions(&self) -> Result<Vec<UDraftSuggestion>, FfError> {
        let strat = *self.strategy.lock().unwrap();
        let snap = self.scheduler.data.read().clone();
        let team_name = snap.roster.as_ref().map(|r| r.team_name.clone()).unwrap_or_default();
        let news = snap.news.clone();
        let provider = self.provider.clone();
        let anthropic = self.anthropic.clone();
        let s = self
            .rt
            .block_on(async move {
                let dm = draft_mod::DraftManager {
                    provider: provider.as_ref(),
                    anthropic: &anthropic,
                    strategy: strat,
                    my_team_name: team_name.clone(),
                    poll_interval: Duration::from_secs(5),
                };
                let state = dm.snapshot().await?;
                let my_picks: Vec<types::DraftPick> = state
                    .picks
                    .iter()
                    .filter(|p| p.team_name.eq_ignore_ascii_case(&team_name))
                    .cloned()
                    .collect();
                dm.ask_claude(&state, &my_picks, &[], &news).await
            })
            .map_err(FfError::from)?;
        Ok(s
            .picks
            .into_iter()
            .map(|p| UDraftSuggestion {
                rank: p.rank,
                name: p.name,
                position: p.position.map(|x| x.to_string()),
                rationale: p.rationale,
            })
            .collect())
    }
}
