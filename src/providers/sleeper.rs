//! Sleeper provider — uses Sleeper's free, no-auth public REST API.
//! https://docs.sleeper.com/
#![allow(dead_code)]

use crate::config::SleeperConfig;
use crate::providers::Provider;
use crate::types::*;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

const BASE: &str = "https://api.sleeper.app/v1";

#[derive(Debug, Deserialize, Clone)]
struct SleeperLeague {
    league_id: String,
    name: String,
    season: String,
    total_rosters: u32,
    settings: HashMap<String, serde_json::Value>,
    roster_positions: Vec<String>,
    scoring_settings: HashMap<String, serde_json::Value>,
    #[serde(default)]
    draft_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct SleeperUser {
    user_id: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, Clone)]
struct SleeperRosterApi {
    roster_id: u32,
    #[serde(default)]
    owner_id: Option<String>,
    #[serde(default)]
    players: Option<Vec<String>>,
    #[serde(default)]
    starters: Option<Vec<String>>,
    #[serde(default)]
    reserve: Option<Vec<String>>,
    #[serde(default)]
    settings: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, Clone)]
struct SleeperPlayer {
    #[serde(default)]
    player_id: String,
    #[serde(default)]
    full_name: Option<String>,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    last_name: Option<String>,
    #[serde(default)]
    position: Option<String>,
    #[serde(default)]
    team: Option<String>,
    #[serde(default)]
    injury_status: Option<String>,
    #[serde(default)]
    injury_notes: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct SleeperMatchup {
    matchup_id: Option<u32>,
    roster_id: u32,
    #[serde(default)]
    points: f32,
    #[serde(default)]
    starters: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
struct SleeperState {
    week: u8,
    season: String,
    season_type: String,
}

#[derive(Debug, Deserialize, Clone)]
struct SleeperDraft {
    draft_id: String,
    #[serde(default)]
    status: String,
    settings: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
struct SleeperDraftPick {
    pick_no: u32,
    round: u32,
    roster_id: Option<u32>,
    picked_by: Option<String>,
    player_id: String,
    metadata: Option<HashMap<String, String>>,
}

pub struct SleeperProvider {
    cfg: SleeperConfig,
    http: Client,
    league: RwLock<Option<SleeperLeague>>,
    players: Arc<RwLock<Option<HashMap<String, SleeperPlayer>>>>,
    users: RwLock<Option<Vec<SleeperUser>>>,
    rosters: RwLock<Option<Vec<SleeperRosterApi>>>,
}

impl SleeperProvider {
    pub fn new(cfg: SleeperConfig) -> Result<Self> {
        if cfg.league_id.is_empty() {
            return Err(anyhow!("sleeper.league_id required"));
        }
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("groks_fantasy/0.2")
            .build()?;
        Ok(Self {
            cfg,
            http,
            league: RwLock::new(None),
            players: Arc::new(RwLock::new(None)),
            users: RwLock::new(None),
            rosters: RwLock::new(None),
        })
    }

    async fn league(&self) -> Result<SleeperLeague> {
        if let Some(l) = self.league.read().await.clone() {
            return Ok(l);
        }
        let url = format!("{BASE}/league/{}", self.cfg.league_id);
        let l: SleeperLeague = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("decode sleeper league")?;
        *self.league.write().await = Some(l.clone());
        Ok(l)
    }

    async fn users(&self) -> Result<Vec<SleeperUser>> {
        if let Some(u) = self.users.read().await.clone() {
            return Ok(u);
        }
        let url = format!("{BASE}/league/{}/users", self.cfg.league_id);
        let u: Vec<SleeperUser> = self.http.get(&url).send().await?.error_for_status()?.json().await?;
        *self.users.write().await = Some(u.clone());
        Ok(u)
    }

    async fn rosters_api(&self) -> Result<Vec<SleeperRosterApi>> {
        if let Some(r) = self.rosters.read().await.clone() {
            return Ok(r);
        }
        let url = format!("{BASE}/league/{}/rosters", self.cfg.league_id);
        let r: Vec<SleeperRosterApi> =
            self.http.get(&url).send().await?.error_for_status()?.json().await?;
        *self.rosters.write().await = Some(r.clone());
        Ok(r)
    }

    async fn all_players(&self) -> Result<HashMap<String, SleeperPlayer>> {
        if let Some(p) = self.players.read().await.clone() {
            return Ok(p);
        }
        let url = format!("{BASE}/players/nfl");
        let p: HashMap<String, SleeperPlayer> =
            self.http.get(&url).send().await?.error_for_status()?.json().await?;
        *self.players.write().await = Some(p.clone());
        Ok(p)
    }

    async fn state(&self) -> Result<SleeperState> {
        let url = format!("{BASE}/state/nfl");
        Ok(self.http.get(&url).send().await?.error_for_status()?.json().await?)
    }

    fn build_player(&self, sp: &SleeperPlayer) -> Player {
        let name = sp
            .full_name
            .clone()
            .or_else(|| match (&sp.first_name, &sp.last_name) {
                (Some(f), Some(l)) => Some(format!("{f} {l}")),
                _ => None,
            })
            .unwrap_or_else(|| sp.player_id.clone());
        let pos = sp
            .position
            .as_deref()
            .map(Position::from_str)
            .unwrap_or(Position::Unknown);
        let status = sp
            .injury_status
            .as_deref()
            .map(PlayerStatus::from_str)
            .unwrap_or(PlayerStatus::Healthy);
        let mut news = Vec::new();
        if let Some(notes) = &sp.injury_notes {
            if !notes.trim().is_empty() {
                news.push(notes.clone());
            }
        }
        Player {
            id: sp.player_id.clone(),
            name,
            position: pos,
            roster_slot: Position::BENCH,
            team: sp.team.clone().unwrap_or_default(),
            projected_points: 0.0,
            avg_points: 0.0,
            status,
            opponent: None,
            bye_week: None,
            news,
        }
    }

    async fn resolve_my_user_id(&self) -> Result<String> {
        if let Some(uid) = &self.cfg.user_id {
            if !uid.is_empty() {
                return Ok(uid.clone());
            }
        }
        if let Some(uname) = &self.cfg.username {
            if !uname.is_empty() {
                let url = format!("{BASE}/user/{}", uname);
                let u: SleeperUser =
                    self.http.get(&url).send().await?.error_for_status()?.json().await?;
                return Ok(u.user_id);
            }
        }
        Err(anyhow!(
            "sleeper config needs user_id or username to identify your team"
        ))
    }
}

#[async_trait]
impl Provider for SleeperProvider {
    fn name(&self) -> &'static str {
        "sleeper"
    }

    async fn league_settings(&self) -> Result<LeagueSettings> {
        let l = self.league().await?;
        let mut counts: HashMap<Position, u32> = HashMap::new();
        for slot in &l.roster_positions {
            *counts.entry(Position::from_str(slot)).or_insert(0) += 1;
        }
        let order = [
            Position::QB,
            Position::RB,
            Position::WR,
            Position::TE,
            Position::FLEX,
            Position::SUPERFLEX,
            Position::DST,
            Position::K,
            Position::BENCH,
            Position::IR,
        ];
        let roster_slots = order
            .into_iter()
            .filter_map(|p| counts.get(&p).map(|c| (p, *c)))
            .collect();
        let scoring = l
            .scoring_settings
            .get("rec")
            .and_then(|v| v.as_f64())
            .map(|v| {
                if v >= 0.99 {
                    "ppr"
                } else if v >= 0.49 {
                    "half_ppr"
                } else {
                    "standard"
                }
            })
            .unwrap_or("ppr");
        Ok(LeagueSettings {
            scoring: scoring.into(),
            roster_slots,
            team_count: l.total_rosters,
        })
    }

    async fn current_week(&self) -> Result<u8> {
        Ok(self.state().await?.week.max(1))
    }

    async fn my_roster(&self) -> Result<Roster> {
        let my_id = self.resolve_my_user_id().await?;
        let rosters = self.rosters_api().await?;
        let users = self.users().await?;
        let players = self.all_players().await?;
        let r = rosters
            .iter()
            .find(|r| r.owner_id.as_deref() == Some(my_id.as_str()))
            .ok_or_else(|| anyhow!("could not find roster for user {my_id}"))?;
        Ok(build_roster(r, &users, &players, |sp| self.build_player(sp)))
    }

    async fn all_rosters(&self) -> Result<Vec<Roster>> {
        let rosters = self.rosters_api().await?;
        let users = self.users().await?;
        let players = self.all_players().await?;
        Ok(rosters
            .iter()
            .map(|r| build_roster(r, &users, &players, |sp| self.build_player(sp)))
            .collect())
    }

    async fn matchups(&self, week: u8) -> Result<Vec<Matchup>> {
        let url = format!(
            "{BASE}/league/{}/matchups/{}",
            self.cfg.league_id, week
        );
        let raw: Vec<SleeperMatchup> =
            self.http.get(&url).send().await?.error_for_status()?.json().await?;
        let rosters = self.rosters_api().await?;
        let users = self.users().await?;
        let team_name = |roster_id: u32| -> String {
            rosters
                .iter()
                .find(|r| r.roster_id == roster_id)
                .and_then(|r| r.owner_id.as_ref())
                .and_then(|oid| users.iter().find(|u| &u.user_id == oid))
                .and_then(|u| u.display_name.clone())
                .unwrap_or_else(|| format!("Team {roster_id}"))
        };
        let mut by_match: HashMap<u32, Vec<&SleeperMatchup>> = HashMap::new();
        for m in &raw {
            if let Some(mid) = m.matchup_id {
                by_match.entry(mid).or_default().push(m);
            }
        }
        let mut out = Vec::new();
        for (_, pair) in by_match {
            if pair.len() == 2 {
                out.push(Matchup {
                    week,
                    home_team: team_name(pair[0].roster_id),
                    away_team: team_name(pair[1].roster_id),
                    home_projected: 0.0,
                    away_projected: 0.0,
                    home_score: pair[0].points,
                    away_score: pair[1].points,
                });
            }
        }
        Ok(out)
    }

    async fn free_agents(&self, position: Option<Position>, limit: usize) -> Result<Vec<Player>> {
        let players = self.all_players().await?;
        let rosters = self.rosters_api().await?;
        let mut taken = std::collections::HashSet::new();
        for r in &rosters {
            if let Some(ps) = &r.players {
                for id in ps {
                    taken.insert(id.clone());
                }
            }
        }
        let mut out: Vec<Player> = players
            .values()
            .filter(|sp| !taken.contains(&sp.player_id))
            .filter(|sp| sp.team.as_deref().map(|t| !t.is_empty()).unwrap_or(false))
            .filter(|sp| match position {
                Some(p) => sp
                    .position
                    .as_deref()
                    .map(|s| Position::from_str(s) == p)
                    .unwrap_or(false),
                None => true,
            })
            .map(|sp| self.build_player(sp))
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out.truncate(limit);
        Ok(out)
    }

    async fn draft_state(&self) -> Result<DraftState> {
        let draft_id = match &self.cfg.draft_id {
            Some(id) if !id.is_empty() => id.clone(),
            _ => self
                .league()
                .await?
                .draft_id
                .ok_or_else(|| anyhow!("league has no associated draft yet"))?,
        };
        let draft: SleeperDraft = self
            .http
            .get(&format!("{BASE}/draft/{draft_id}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let picks: Vec<SleeperDraftPick> = self
            .http
            .get(&format!("{BASE}/draft/{draft_id}/picks"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let players = self.all_players().await?;
        let users = self.users().await?;
        let total_rounds = draft
            .settings
            .get("rounds")
            .and_then(|v| v.as_u64())
            .unwrap_or(15) as u32;
        let team_count = draft
            .settings
            .get("teams")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32;
        let pick_list: Vec<DraftPick> = picks
            .iter()
            .map(|p| {
                let player = players.get(&p.player_id);
                let team = p
                    .picked_by
                    .as_ref()
                    .and_then(|uid| users.iter().find(|u| &u.user_id == uid))
                    .and_then(|u| u.display_name.clone())
                    .unwrap_or_else(|| format!("Team {:?}", p.roster_id));
                DraftPick {
                    pick_number: p.pick_no,
                    round: p.round,
                    team_id: p.roster_id.map(|r| r.to_string()).unwrap_or_default(),
                    team_name: team,
                    player_id: Some(p.player_id.clone()),
                    player_name: player.map(|sp| {
                        sp.full_name.clone().unwrap_or_else(|| {
                            format!(
                                "{} {}",
                                sp.first_name.clone().unwrap_or_default(),
                                sp.last_name.clone().unwrap_or_default()
                            )
                        })
                    }),
                    position: player
                        .and_then(|sp| sp.position.as_deref())
                        .map(Position::from_str),
                }
            })
            .collect();
        let next_pick = pick_list.last().map(|p| p.pick_number + 1).unwrap_or(1);
        let completed = draft.status == "complete";
        Ok(DraftState {
            picks: pick_list,
            current_pick: next_pick,
            on_the_clock_team: None,
            total_rounds,
            team_count,
            completed,
        })
    }
}

fn build_roster(
    r: &SleeperRosterApi,
    users: &[SleeperUser],
    players: &HashMap<String, SleeperPlayer>,
    build_player: impl Fn(&SleeperPlayer) -> Player,
) -> Roster {
    let owner = r
        .owner_id
        .as_ref()
        .and_then(|oid| users.iter().find(|u| &u.user_id == oid));
    let team_name = owner
        .and_then(|u| u.metadata.as_ref())
        .and_then(|m| m.get("team_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| owner.and_then(|u| u.display_name.clone()))
        .unwrap_or_else(|| format!("Team {}", r.roster_id));
    let starters: Vec<&str> = r
        .starters
        .as_ref()
        .map(|s| s.iter().map(|x| x.as_str()).collect())
        .unwrap_or_default();
    let mut roster_players = Vec::new();
    if let Some(ids) = &r.players {
        for id in ids {
            if id == "0" {
                continue;
            }
            if let Some(sp) = players.get(id) {
                let mut p = build_player(sp);
                if starters.contains(&id.as_str()) {
                    p.roster_slot = p.position;
                }
                roster_players.push(p);
            }
        }
    }
    let (wins, losses, ties, pf, pa) = r
        .settings
        .as_ref()
        .map(|s| {
            (
                s.get("wins").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                s.get("losses").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                s.get("ties").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                s.get("fpts").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                s.get("fpts_against").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            )
        })
        .unwrap_or((0, 0, 0, 0.0, 0.0));
    Roster {
        team_id: r.roster_id.to_string(),
        team_name,
        owner: owner.and_then(|u| u.display_name.clone()),
        players: roster_players,
        wins,
        losses,
        ties,
        points_for: pf,
        points_against: pa,
    }
}
