//! ESPN provider — uses the (undocumented) lm-api endpoints with cookie auth
//! for private leagues. Read-only; lineup edits are surfaced to the user.
#![allow(dead_code)]

use crate::config::EspnConfig;
use crate::providers::Provider;
use crate::types::*;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::header;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

fn base_url(season: u32, league_id: u64) -> String {
    format!(
        "https://lm-api-reads.fantasy.espn.com/apis/v3/games/ffl/seasons/{season}/segments/0/leagues/{league_id}"
    )
}

#[derive(Debug, Clone, Deserialize, Default)]
struct LeagueResponse {
    #[serde(default)]
    settings: Option<LeagueSettingsApi>,
    #[serde(default)]
    teams: Vec<TeamApi>,
    #[serde(default)]
    schedule: Vec<ScheduleEntry>,
    #[serde(default, rename = "scoringPeriodId")]
    scoring_period_id: Option<u8>,
    #[serde(default)]
    status: Option<StatusApi>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct StatusApi {
    #[serde(default, rename = "currentMatchupPeriod")]
    current_matchup_period: Option<u8>,
    #[serde(default, rename = "latestScoringPeriod")]
    latest_scoring_period: Option<u8>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct LeagueSettingsApi {
    #[serde(default, rename = "rosterSettings")]
    roster_settings: Option<RosterSettingsApi>,
    #[serde(default, rename = "scoringSettings")]
    scoring_settings: Option<ScoringSettingsApi>,
    #[serde(default, rename = "size")]
    size: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RosterSettingsApi {
    #[serde(default, rename = "lineupSlotCounts")]
    lineup_slot_counts: HashMap<String, u32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ScoringSettingsApi {
    #[serde(default, rename = "scoringType")]
    scoring_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct TeamApi {
    id: u32,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    nickname: Option<String>,
    #[serde(default)]
    abbrev: Option<String>,
    #[serde(default)]
    record: Option<RecordApi>,
    #[serde(default)]
    roster: Option<TeamRosterApi>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RecordApi {
    #[serde(default)]
    overall: Option<RecordOverall>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RecordOverall {
    #[serde(default)]
    wins: u32,
    #[serde(default)]
    losses: u32,
    #[serde(default)]
    ties: u32,
    #[serde(default, rename = "pointsFor")]
    points_for: f32,
    #[serde(default, rename = "pointsAgainst")]
    points_against: f32,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct TeamRosterApi {
    #[serde(default)]
    entries: Vec<RosterEntry>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RosterEntry {
    #[serde(default, rename = "playerId")]
    player_id: u64,
    #[serde(default, rename = "lineupSlotId")]
    lineup_slot_id: u32,
    #[serde(default, rename = "playerPoolEntry")]
    player_pool_entry: Option<PlayerPoolEntry>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PlayerPoolEntry {
    #[serde(default)]
    player: Option<PlayerApi>,
    #[serde(default, rename = "appliedStatTotal")]
    applied_stat_total: Option<f32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PlayerApi {
    id: u64,
    #[serde(default, rename = "fullName")]
    full_name: String,
    #[serde(default, rename = "defaultPositionId")]
    default_position_id: u32,
    #[serde(default, rename = "proTeamId")]
    pro_team_id: u32,
    #[serde(default, rename = "injuryStatus")]
    injury_status: Option<String>,
    #[serde(default)]
    stats: Vec<StatBlock>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct StatBlock {
    #[serde(default, rename = "statSourceId")]
    stat_source_id: u32,
    #[serde(default, rename = "scoringPeriodId")]
    scoring_period_id: u32,
    #[serde(default, rename = "appliedTotal")]
    applied_total: f32,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ScheduleEntry {
    #[serde(default, rename = "matchupPeriodId")]
    matchup_period_id: u8,
    #[serde(default)]
    home: Option<ScheduleSide>,
    #[serde(default)]
    away: Option<ScheduleSide>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ScheduleSide {
    #[serde(default, rename = "teamId")]
    team_id: u32,
    #[serde(default, rename = "totalPoints")]
    total_points: f32,
    #[serde(default, rename = "totalProjectedPointsLive")]
    total_projected_points: f32,
}

fn slot_id_to_position(id: u32) -> Position {
    match id {
        0 => Position::QB,
        2 => Position::RB,
        4 => Position::WR,
        6 => Position::TE,
        16 => Position::DST,
        17 => Position::K,
        20 => Position::BENCH,
        21 => Position::IR,
        23 => Position::FLEX,
        7 => Position::SUPERFLEX,
        _ => Position::Unknown,
    }
}

fn default_position_id_to_position(id: u32) -> Position {
    match id {
        1 => Position::QB,
        2 => Position::RB,
        3 => Position::WR,
        4 => Position::TE,
        5 => Position::K,
        16 => Position::DST,
        _ => Position::Unknown,
    }
}

fn pro_team(id: u32) -> &'static str {
    match id {
        0 => "FA",
        1 => "ATL",
        2 => "BUF",
        3 => "CHI",
        4 => "CIN",
        5 => "CLE",
        6 => "DAL",
        7 => "DEN",
        8 => "DET",
        9 => "GB",
        10 => "TEN",
        11 => "IND",
        12 => "KC",
        13 => "LV",
        14 => "LAR",
        15 => "MIA",
        16 => "MIN",
        17 => "NE",
        18 => "NO",
        19 => "NYG",
        20 => "NYJ",
        21 => "PHI",
        22 => "ARI",
        23 => "PIT",
        24 => "LAC",
        25 => "SF",
        26 => "SEA",
        27 => "TB",
        28 => "WSH",
        29 => "CAR",
        30 => "JAX",
        33 => "BAL",
        34 => "HOU",
        _ => "",
    }
}

pub struct EspnProvider {
    cfg: EspnConfig,
    http: Client,
    cache: Arc<RwLock<Option<LeagueResponse>>>,
}

impl EspnProvider {
    pub fn new(cfg: EspnConfig) -> Result<Self> {
        if cfg.league_id == 0 {
            return Err(anyhow!("espn.league_id required"));
        }
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            ),
        );
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );
        if !cfg.swid.is_empty() && !cfg.espn_s2.is_empty() {
            let cookie = format!("SWID={}; espn_s2={}", cfg.swid, cfg.espn_s2);
            headers.insert(header::COOKIE, header::HeaderValue::from_str(&cookie)?);
        }
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .default_headers(headers)
            .build()?;
        Ok(Self {
            cfg,
            http,
            cache: Arc::new(RwLock::new(None)),
        })
    }

    async fn fetch_league(&self, views: &[&str]) -> Result<LeagueResponse> {
        let url = base_url(self.cfg.season, self.cfg.league_id);
        let mut req = self.http.get(&url);
        for v in views {
            req = req.query(&[("view", *v)]);
        }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "ESPN API {} (private league? check swid/espn_s2): {}",
                status,
                truncate(&body, 200)
            ));
        }
        let parsed: LeagueResponse = resp.json().await.context("decode espn response")?;
        Ok(parsed)
    }

    async fn cached_league(&self) -> Result<LeagueResponse> {
        if let Some(c) = self.cache.read().await.clone() {
            return Ok(c);
        }
        let l = self
            .fetch_league(&["mTeam", "mRoster", "mSettings", "mSchedule", "mMatchupScore"])
            .await?;
        *self.cache.write().await = Some(l.clone());
        Ok(l)
    }

    fn build_player(&self, entry: &RosterEntry) -> Option<Player> {
        let pool = entry.player_pool_entry.as_ref()?;
        let p = pool.player.as_ref()?;
        let pos = default_position_id_to_position(p.default_position_id);
        let slot = slot_id_to_position(entry.lineup_slot_id);
        let status = p
            .injury_status
            .as_deref()
            .map(PlayerStatus::from_str)
            .unwrap_or(PlayerStatus::Healthy);
        let projected = p
            .stats
            .iter()
            .find(|s| s.stat_source_id == 1)
            .map(|s| s.applied_total)
            .unwrap_or(0.0);
        let avg = pool.applied_stat_total.unwrap_or(0.0);
        Some(Player {
            id: p.id.to_string(),
            name: p.full_name.clone(),
            position: pos,
            roster_slot: slot,
            team: pro_team(p.pro_team_id).to_string(),
            projected_points: projected,
            avg_points: avg,
            status,
            opponent: None,
            bye_week: None,
            news: vec![],
        })
    }

    fn build_team(&self, t: &TeamApi) -> Roster {
        let team_name = match (&t.location, &t.nickname) {
            (Some(l), Some(n)) => format!("{l} {n}").trim().to_string(),
            _ => t.name.clone().unwrap_or_else(|| format!("Team {}", t.id)),
        };
        let players = t
            .roster
            .as_ref()
            .map(|r| {
                r.entries
                    .iter()
                    .filter_map(|e| self.build_player(e))
                    .collect()
            })
            .unwrap_or_default();
        let r = t.record.as_ref().and_then(|r| r.overall.as_ref());
        Roster {
            team_id: t.id.to_string(),
            team_name,
            owner: None,
            players,
            wins: r.map(|r| r.wins).unwrap_or(0),
            losses: r.map(|r| r.losses).unwrap_or(0),
            ties: r.map(|r| r.ties).unwrap_or(0),
            points_for: r.map(|r| r.points_for).unwrap_or(0.0),
            points_against: r.map(|r| r.points_against).unwrap_or(0.0),
        }
    }
}

#[async_trait]
impl Provider for EspnProvider {
    fn name(&self) -> &'static str {
        "espn"
    }

    async fn league_settings(&self) -> Result<LeagueSettings> {
        let l = self.cached_league().await?;
        let scoring = l
            .settings
            .as_ref()
            .and_then(|s| s.scoring_settings.as_ref())
            .and_then(|s| s.scoring_type.clone())
            .unwrap_or_else(|| "ppr".into());
        let team_count = l.settings.as_ref().and_then(|s| s.size).unwrap_or(10);
        let mut slots = Vec::new();
        if let Some(rs) = l.settings.as_ref().and_then(|s| s.roster_settings.as_ref()) {
            for (k, v) in &rs.lineup_slot_counts {
                if let Ok(id) = k.parse::<u32>() {
                    let pos = slot_id_to_position(id);
                    if pos != Position::Unknown && *v > 0 {
                        slots.push((pos, *v));
                    }
                }
            }
        }
        Ok(LeagueSettings {
            scoring,
            roster_slots: slots,
            team_count,
        })
    }

    async fn current_week(&self) -> Result<u8> {
        let l = self.cached_league().await?;
        Ok(l.scoring_period_id
            .or_else(|| l.status.as_ref().and_then(|s| s.current_matchup_period))
            .unwrap_or(1)
            .max(1))
    }

    async fn my_roster(&self) -> Result<Roster> {
        let l = self.cached_league().await?;
        let team_id = self.cfg.team_id.ok_or_else(|| {
            anyhow!("espn.team_id required to identify your roster (find it on the My Team page)")
        })?;
        let team = l
            .teams
            .iter()
            .find(|t| t.id == team_id)
            .ok_or_else(|| anyhow!("team_id {team_id} not found in league"))?;
        Ok(self.build_team(team))
    }

    async fn all_rosters(&self) -> Result<Vec<Roster>> {
        let l = self.cached_league().await?;
        Ok(l.teams.iter().map(|t| self.build_team(t)).collect())
    }

    async fn matchups(&self, week: u8) -> Result<Vec<Matchup>> {
        let l = self.cached_league().await?;
        let teams: HashMap<u32, &TeamApi> = l.teams.iter().map(|t| (t.id, t)).collect();
        let resolve = |id: u32| -> String {
            teams
                .get(&id)
                .map(|t| match (&t.location, &t.nickname) {
                    (Some(l), Some(n)) => format!("{l} {n}").trim().to_string(),
                    _ => t.name.clone().unwrap_or_else(|| format!("Team {id}")),
                })
                .unwrap_or_else(|| format!("Team {id}"))
        };
        Ok(l.schedule
            .iter()
            .filter(|m| m.matchup_period_id == week)
            .filter_map(|m| {
                let h = m.home.as_ref()?;
                let a = m.away.as_ref()?;
                Some(Matchup {
                    week,
                    home_team: resolve(h.team_id),
                    away_team: resolve(a.team_id),
                    home_projected: h.total_projected_points,
                    away_projected: a.total_projected_points,
                    home_score: h.total_points,
                    away_score: a.total_points,
                })
            })
            .collect())
    }

    async fn free_agents(&self, _position: Option<Position>, _limit: usize) -> Result<Vec<Player>> {
        // ESPN's player pool requires a paged kona endpoint with x-fantasy-filter header.
        // Skipped for now; downstream code tolerates an empty list.
        Ok(vec![])
    }

    async fn draft_state(&self) -> Result<DraftState> {
        let url = base_url(self.cfg.season, self.cfg.league_id);
        let resp = self
            .http
            .get(&url)
            .query(&[("view", "mDraftDetail")])
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(anyhow!("ESPN draft endpoint {}", resp.status()));
        }
        #[derive(Deserialize, Default)]
        struct Wrap {
            #[serde(default, rename = "draftDetail")]
            draft_detail: Option<DraftDetail>,
        }
        #[derive(Deserialize, Default)]
        struct DraftDetail {
            #[serde(default)]
            picks: Vec<DraftPickApi>,
            #[serde(default)]
            drafted: bool,
            #[serde(default, rename = "inProgress")]
            in_progress: bool,
        }
        #[derive(Deserialize, Default)]
        struct DraftPickApi {
            #[serde(default, rename = "overallPickNumber")]
            overall_pick_number: u32,
            #[serde(default, rename = "roundId")]
            round_id: u32,
            #[serde(default, rename = "teamId")]
            team_id: u32,
            #[serde(default, rename = "playerId")]
            player_id: u64,
        }
        let w: Wrap = resp.json().await?;
        let dd = w.draft_detail.unwrap_or_default();
        let l = self.cached_league().await?;
        let teams: HashMap<u32, &TeamApi> = l.teams.iter().map(|t| (t.id, t)).collect();
        let player_lookup: HashMap<u64, PlayerApi> = l
            .teams
            .iter()
            .flat_map(|t| {
                t.roster
                    .as_ref()
                    .map(|r| r.entries.iter())
                    .into_iter()
                    .flatten()
            })
            .filter_map(|e| {
                e.player_pool_entry
                    .as_ref()
                    .and_then(|p| p.player.clone())
            })
            .map(|p| (p.id, p))
            .collect();
        let total_rounds = dd.picks.iter().map(|p| p.round_id).max().unwrap_or(15);
        let picks: Vec<DraftPick> = dd
            .picks
            .iter()
            .map(|p| {
                let team_name = teams
                    .get(&p.team_id)
                    .map(|t| match (&t.location, &t.nickname) {
                        (Some(l), Some(n)) => format!("{l} {n}").trim().to_string(),
                        _ => t.name.clone().unwrap_or_else(|| format!("Team {}", p.team_id)),
                    })
                    .unwrap_or_else(|| format!("Team {}", p.team_id));
                let player = player_lookup.get(&p.player_id);
                DraftPick {
                    pick_number: p.overall_pick_number,
                    round: p.round_id,
                    team_id: p.team_id.to_string(),
                    team_name,
                    player_id: Some(p.player_id.to_string()),
                    player_name: player.map(|pl| pl.full_name.clone()),
                    position: player
                        .map(|pl| default_position_id_to_position(pl.default_position_id)),
                }
            })
            .collect();
        let next_pick = picks.last().map(|p| p.pick_number + 1).unwrap_or(1);
        let team_count = l.settings.as_ref().and_then(|s| s.size).unwrap_or(10);
        Ok(DraftState {
            picks,
            current_pick: next_pick,
            on_the_clock_team: None,
            total_rounds,
            team_count,
            completed: dd.drafted && !dd.in_progress,
        })
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
