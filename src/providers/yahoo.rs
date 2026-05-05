//! Yahoo Fantasy provider.
//!
//! Yahoo's fantasy API requires OAuth2 (3-legged). This provider expects the
//! user to supply an already-issued access token (refreshed externally if
//! desired). It is more limited than ESPN/Sleeper because Yahoo returns XML
//! by default and OAuth flow is out-of-scope for an offline tool.
//!
//! Endpoint reference: https://developer.yahoo.com/fantasysports/guide/

use crate::config::YahooConfig;
use crate::providers::Provider;
use crate::types::*;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

const BASE: &str = "https://fantasysports.yahooapis.com/fantasy/v2";

pub struct YahooProvider {
    cfg: YahooConfig,
    http: Client,
}

impl YahooProvider {
    pub fn new(cfg: YahooConfig) -> Result<Self> {
        if cfg.league_key.is_empty() {
            return Err(anyhow!("yahoo.league_key required (e.g. 423.l.123456)"));
        }
        if cfg.access_token.is_empty() {
            return Err(anyhow!(
                "yahoo.access_token required — run an OAuth2 helper to issue one"
            ));
        }
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("groks_fantasy/0.2")
            .build()?;
        Ok(Self { cfg, http })
    }

    async fn get(&self, path: &str) -> Result<String> {
        let url = format!("{BASE}/{path}?format=json");
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.cfg.access_token)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Yahoo API {} (token expired? refresh or reissue): {}",
                status,
                truncate(&body, 200)
            ));
        }
        Ok(resp.text().await?)
    }
}

#[async_trait]
impl Provider for YahooProvider {
    fn name(&self) -> &'static str {
        "yahoo"
    }

    async fn league_settings(&self) -> Result<LeagueSettings> {
        let body = self
            .get(&format!("league/{}/settings", self.cfg.league_key))
            .await?;
        let v: serde_json::Value = serde_json::from_str(&body).context("yahoo settings json")?;
        let scoring = v
            .pointer("/fantasy_content/league/0/scoring_type")
            .and_then(|x| x.as_str())
            .unwrap_or("ppr")
            .to_string();
        let team_count = v
            .pointer("/fantasy_content/league/0/num_teams")
            .and_then(|x| x.as_u64())
            .unwrap_or(10) as u32;
        Ok(LeagueSettings {
            scoring,
            roster_slots: LeagueSettings::default().roster_slots,
            team_count,
        })
    }

    async fn current_week(&self) -> Result<u8> {
        let body = self.get(&format!("league/{}", self.cfg.league_key)).await?;
        let v: serde_json::Value = serde_json::from_str(&body)?;
        Ok(v.pointer("/fantasy_content/league/0/current_week")
            .and_then(|x| x.as_u64())
            .unwrap_or(1) as u8)
    }

    async fn my_roster(&self) -> Result<Roster> {
        let team_key = self
            .cfg
            .team_key
            .as_ref()
            .ok_or_else(|| anyhow!("yahoo.team_key required to fetch your roster"))?;
        let body = self.get(&format!("team/{team_key}/roster/players")).await?;
        let v: serde_json::Value = serde_json::from_str(&body)?;
        let team_name = v
            .pointer("/fantasy_content/team/0/name")
            .and_then(|x| x.as_str())
            .unwrap_or("My Team")
            .to_string();
        let players = parse_yahoo_players(&v);
        Ok(Roster {
            team_id: team_key.clone(),
            team_name,
            owner: None,
            players,
            wins: 0,
            losses: 0,
            ties: 0,
            points_for: 0.0,
            points_against: 0.0,
        })
    }

    async fn all_rosters(&self) -> Result<Vec<Roster>> {
        let body = self
            .get(&format!("league/{}/teams", self.cfg.league_key))
            .await?;
        let v: serde_json::Value = serde_json::from_str(&body)?;
        let teams = v
            .pointer("/fantasy_content/league/1/teams")
            .and_then(|t| t.as_object())
            .map(|o| {
                o.values()
                    .filter_map(|t| t.pointer("/team/0"))
                    .filter_map(|t| {
                        t.as_array().map(|arr| {
                            let key = arr
                                .iter()
                                .find_map(|x| x.get("team_key").and_then(|v| v.as_str()))
                                .unwrap_or("")
                                .to_string();
                            let name = arr
                                .iter()
                                .find_map(|x| x.get("name").and_then(|v| v.as_str()))
                                .unwrap_or("")
                                .to_string();
                            Roster {
                                team_id: key,
                                team_name: name,
                                owner: None,
                                players: vec![],
                                wins: 0,
                                losses: 0,
                                ties: 0,
                                points_for: 0.0,
                                points_against: 0.0,
                            }
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(teams)
    }

    async fn matchups(&self, week: u8) -> Result<Vec<Matchup>> {
        let body = self
            .get(&format!(
                "league/{}/scoreboard;week={}",
                self.cfg.league_key, week
            ))
            .await?;
        let _: serde_json::Value = serde_json::from_str(&body)?;
        // Yahoo's response shape is irregular; return empty rather than fabricate.
        Ok(vec![])
    }

    async fn free_agents(&self, _position: Option<Position>, _limit: usize) -> Result<Vec<Player>> {
        Ok(vec![])
    }

    async fn draft_state(&self) -> Result<DraftState> {
        let body = self
            .get(&format!("league/{}/draftresults", self.cfg.league_key))
            .await?;
        let _: serde_json::Value = serde_json::from_str(&body)?;
        Ok(DraftState {
            picks: vec![],
            current_pick: 1,
            on_the_clock_team: None,
            total_rounds: 15,
            team_count: 10,
            completed: false,
        })
    }
}

#[derive(Debug, Deserialize)]
struct YahooName {
    full: Option<String>,
}

fn parse_yahoo_players(v: &serde_json::Value) -> Vec<Player> {
    let mut out = Vec::new();
    let players = match v.pointer("/fantasy_content/team/1/roster/0/players") {
        Some(serde_json::Value::Object(map)) => map,
        _ => return out,
    };
    for (k, entry) in players {
        if k == "count" {
            continue;
        }
        let arr = match entry.pointer("/player/0") {
            Some(serde_json::Value::Array(a)) => a,
            _ => continue,
        };
        let mut name = None;
        let mut pos = Position::Unknown;
        let mut team = String::new();
        let mut id = String::new();
        let mut status = PlayerStatus::Healthy;
        for item in arr {
            if let Some(n) = item.get("name") {
                if let Ok(yn) = serde_json::from_value::<YahooName>(n.clone()) {
                    name = yn.full;
                }
            }
            if let Some(p) = item.get("display_position").and_then(|x| x.as_str()) {
                pos = Position::from_str(p);
            }
            if let Some(t) = item.get("editorial_team_abbr").and_then(|x| x.as_str()) {
                team = t.to_string();
            }
            if let Some(pid) = item.get("player_id").and_then(|x| x.as_str()) {
                id = pid.to_string();
            }
            if let Some(s) = item.get("status").and_then(|x| x.as_str()) {
                status = PlayerStatus::from_str(s);
            }
        }
        let slot = entry
            .pointer("/player/1/selected_position/0/position")
            .and_then(|x| x.as_str())
            .map(Position::from_str)
            .unwrap_or(Position::BENCH);
        out.push(Player {
            id,
            name: name.unwrap_or_default(),
            position: pos,
            roster_slot: slot,
            team,
            projected_points: 0.0,
            avg_points: 0.0,
            status,
            opponent: None,
            bye_week: None,
            news: vec![],
        });
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
