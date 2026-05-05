use crate::anthropic::Anthropic;
use crate::metrics::PlayerMetrics;
use crate::news::NewsItem;
use crate::providers::Provider;
use crate::strategy::Strategy;
use crate::types::*;
use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct WaiverCandidate {
    pub priority: u32,
    pub player: Player,
    pub metrics: PlayerMetrics,
    pub drop_candidate: Option<DropCandidate>,
    pub reasoning: String,
}

#[derive(Debug, Clone)]
pub struct DropCandidate {
    pub player: Player,
    pub metrics: PlayerMetrics,
    pub net_ros_delta: f32,
}

#[derive(Debug, Clone)]
pub struct WaiverReport {
    pub candidates: Vec<WaiverCandidate>,
    pub raw: String,
}

pub async fn analyze(
    provider: &dyn Provider,
    anthropic: &Anthropic,
    strategy: Strategy,
    news: &[NewsItem],
    free_agent_limit: usize,
) -> Result<WaiverReport> {
    let roster = provider.my_roster().await?;
    let free_agents = provider.free_agents(None, free_agent_limit).await?;
    let roster_metrics: Vec<PlayerMetrics> = roster
        .players
        .iter()
        .map(|p| PlayerMetrics::for_player(p, strategy))
        .collect();
    let fa_metrics: Vec<PlayerMetrics> = free_agents
        .iter()
        .map(|p| PlayerMetrics::for_player(p, strategy))
        .collect();

    let candidates = if free_agents.is_empty() {
        // Provider doesn't expose free agents (e.g. ESPN stub). Defer to AI.
        ai_only_candidates(anthropic, &roster, &roster_metrics, news, strategy).await?
    } else {
        local_rank(&free_agents, &fa_metrics, &roster, &roster_metrics, strategy)
    };

    let raw = ai_polish(
        anthropic,
        &roster,
        &candidates,
        news,
        strategy,
    )
    .await
    .unwrap_or_default();

    Ok(WaiverReport { candidates, raw })
}

fn local_rank(
    free_agents: &[Player],
    fa_metrics: &[PlayerMetrics],
    roster: &Roster,
    roster_metrics: &[PlayerMetrics],
    _strategy: Strategy,
) -> Vec<WaiverCandidate> {
    let weakest_at: HashMap<Position, &PlayerMetrics> = {
        let mut map: HashMap<Position, &PlayerMetrics> = HashMap::new();
        for (p, m) in roster.players.iter().zip(roster_metrics.iter()) {
            // ignore IR / suspended slots when looking for drops
            if matches!(p.position, Position::IR | Position::Unknown) {
                continue;
            }
            map.entry(p.position)
                .and_modify(|cur| {
                    if m.ros_value < cur.ros_value {
                        *cur = m;
                    }
                })
                .or_insert(m);
        }
        map
    };

    let mut paired: Vec<(usize, f32)> = fa_metrics
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let weakest = weakest_at.get(&m.position).copied();
            let delta = match weakest {
                Some(w) => m.ros_value - w.ros_value,
                None => m.ros_value, // we have no one at this position
            };
            (i, delta * (0.5 + 0.5 * m.strategy_fit))
        })
        .collect();
    paired.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    paired
        .into_iter()
        .filter(|(_, score)| *score > 0.0)
        .take(8)
        .enumerate()
        .map(|(rank, (idx, score))| {
            let player = free_agents[idx].clone();
            let metrics = fa_metrics[idx].clone();
            let drop_candidate = weakest_at.get(&metrics.position).map(|w| {
                let p = roster
                    .players
                    .iter()
                    .find(|rp| rp.id == w.player_id)
                    .cloned()
                    .unwrap();
                DropCandidate {
                    metrics: (*w).clone(),
                    player: p,
                    net_ros_delta: metrics.ros_value - w.ros_value,
                }
            });
            WaiverCandidate {
                priority: (rank + 1) as u32,
                player,
                metrics,
                drop_candidate,
                reasoning: format!("ROS upgrade vs your {} (score {:.1})", metrics_pos_str(idx, fa_metrics), score),
            }
        })
        .collect()
}

fn metrics_pos_str(idx: usize, all: &[PlayerMetrics]) -> String {
    all.get(idx)
        .map(|m| m.position.to_string())
        .unwrap_or_else(|| "?".into())
}

async fn ai_only_candidates(
    anthropic: &Anthropic,
    roster: &Roster,
    roster_metrics: &[PlayerMetrics],
    news: &[NewsItem],
    strategy: Strategy,
) -> Result<Vec<WaiverCandidate>> {
    // Without a free-agent pool, we ask Claude for candidates by name.
    let system = format!(
        "You are an autonomous fantasy football GM. {}\n\
         Recommend the top 5 likely-available waiver pickups for a typical \
         league. Do not suggest players already on the user's roster. Each \
         pickup should target a positional weakness and explain why now.",
        strategy.guidance()
    );
    let roster_block = roster
        .players
        .iter()
        .zip(roster_metrics.iter())
        .map(|(p, m)| format!("- {} ({}, {}) {}", p.name, p.position, p.team, m.one_line()))
        .collect::<Vec<_>>()
        .join("\n");
    let news_block = news
        .iter()
        .take(15)
        .map(|n| format!("- [{}] {}", n.source, n.title))
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        "Roster (with metrics):\n{roster_block}\n\nRecent news:\n{news_block}\n\n\
         Respond with exactly 5 lines:\n\
         1. <Name> (<POS>) — <one-line reasoning>\n\
         (use 1-5 numbering)"
    );
    let raw = anthropic.complete(&system, &user).await?;
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        let Some(rest) = (1..=9)
            .find_map(|n| t.strip_prefix(&format!("{n}.")))
        else {
            continue;
        };
        let rest = rest.trim();
        let (name_part, reasoning) = match rest.split_once('—').or_else(|| rest.split_once('-')) {
            Some((n, r)) => (n.trim().to_string(), r.trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
        let (name, position) = match name_part.rsplit_once('(') {
            Some((n, p)) => {
                let pos = Position::from_str(p.trim_end_matches(')').trim());
                (n.trim().to_string(), pos)
            }
            None => (name_part, Position::Unknown),
        };
        let player = Player {
            id: format!("ai:{name}"),
            name: name.clone(),
            position,
            roster_slot: Position::BENCH,
            team: String::new(),
            projected_points: 0.0,
            avg_points: 0.0,
            status: PlayerStatus::Healthy,
            opponent: None,
            bye_week: None,
            news: vec![],
        };
        let metrics = PlayerMetrics::for_player(&player, strategy);
        out.push(WaiverCandidate {
            priority: (out.len() + 1) as u32,
            player,
            metrics,
            drop_candidate: None,
            reasoning,
        });
        if out.len() >= 5 {
            break;
        }
    }
    Ok(out)
}

async fn ai_polish(
    anthropic: &Anthropic,
    roster: &Roster,
    candidates: &[WaiverCandidate],
    news: &[NewsItem],
    strategy: Strategy,
) -> Result<String> {
    if candidates.is_empty() {
        return Ok(String::new());
    }
    let system = format!(
        "You are an autonomous fantasy football GM polishing a waiver report. {}\n\
         Re-rank or confirm the candidate list and add a one-paragraph summary \
         of the recommended waiver moves and their expected impact.",
        strategy.guidance()
    );
    let cand_block = candidates
        .iter()
        .map(|c| {
            let drop = c
                .drop_candidate
                .as_ref()
                .map(|d| {
                    format!(
                        "drop {} (ROS {:.0}, Δ {:+.0})",
                        d.player.name, d.metrics.ros_value, d.net_ros_delta
                    )
                })
                .unwrap_or_else(|| "no drop suggested".into());
            format!(
                "{}. {} ({}, {}) — {} | {}",
                c.priority, c.player.name, c.player.position, c.player.team,
                c.metrics.one_line(),
                drop
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let roster_names = roster
        .players
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let news_block = news
        .iter()
        .take(10)
        .map(|n| format!("- [{}] {}", n.source, n.title))
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        "Roster: {roster_names}\n\nCandidates with metrics:\n{cand_block}\n\n\
         Recent news:\n{news_block}\n\n\
         Provide a 2-4 sentence summary, then a re-ranked top 5 with a short \
         rationale per pickup."
    );
    anthropic.complete(&system, &user).await
}
