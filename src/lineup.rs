use crate::anthropic::Anthropic;
use crate::news::NewsItem;
use crate::strategy::Strategy;
use crate::types::*;
use anyhow::Result;
use std::collections::HashMap;

/// Adjust a player's projected points based on injury status and chosen strategy.
pub fn risk_adjusted(player: &Player, strat: Strategy) -> f32 {
    let base = if player.projected_points > 0.0 {
        player.projected_points
    } else {
        player.avg_points
    };
    let mult = match player.status {
        PlayerStatus::Healthy => 1.0,
        PlayerStatus::Questionable => strat.questionable_multiplier(),
        PlayerStatus::Doubtful => strat.doubtful_multiplier(),
        PlayerStatus::Out | PlayerStatus::IR | PlayerStatus::Suspended => 0.0,
        PlayerStatus::Unknown => 0.95,
    };
    base * mult
}

/// Greedy local optimizer that fills required slots in order, using
/// risk-adjusted projections. Used as a deterministic fallback when the
/// AI call fails or as a baseline shown alongside the AI recommendation.
pub fn local_optimize(roster: &Roster, settings: &LeagueSettings, strat: Strategy) -> Lineup {
    let mut available: Vec<&Player> = roster.players.iter().collect();
    available.sort_by(|a, b| {
        risk_adjusted(b, strat)
            .partial_cmp(&risk_adjusted(a, strat))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut starters: Vec<LineupSlot> = Vec::new();
    let mut chosen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Fill non-flex slots first so flex doesn't grab a top RB and starve RB1.
    let priority = [
        Position::QB,
        Position::RB,
        Position::WR,
        Position::TE,
        Position::K,
        Position::DST,
        Position::FLEX,
        Position::SUPERFLEX,
    ];
    for slot in priority {
        let count = settings
            .roster_slots
            .iter()
            .find_map(|(p, c)| if *p == slot { Some(*c) } else { None })
            .unwrap_or(0);
        for _ in 0..count {
            if let Some((idx, _)) = available
                .iter()
                .enumerate()
                .find(|(_, p)| !chosen_ids.contains(&p.id) && p.eligible_for(slot))
            {
                let p = available[idx].clone();
                chosen_ids.insert(p.id.clone());
                starters.push(LineupSlot {
                    slot,
                    player: Some(p),
                });
            } else {
                starters.push(LineupSlot { slot, player: None });
            }
        }
    }

    let bench: Vec<Player> = roster
        .players
        .iter()
        .filter(|p| !chosen_ids.contains(&p.id))
        .cloned()
        .collect();

    let projected_total: f32 = starters
        .iter()
        .filter_map(|s| s.player.as_ref().map(|p| risk_adjusted(p, strat)))
        .sum();

    Lineup {
        week: 0,
        starters,
        bench,
        projected_total,
        reasoning: format!(
            "Greedy {} fallback — slots filled in priority order by risk-adjusted projection.",
            strat.label()
        ),
    }
}

/// Ask Claude to refine the local recommendation with news context. Returns
/// a (Lineup, raw_reasoning) pair. On API failure, the local optimizer's
/// result is returned with the error noted in reasoning.
pub async fn ai_optimize(
    client: &Anthropic,
    roster: &Roster,
    settings: &LeagueSettings,
    matchups: &[Matchup],
    news: &[NewsItem],
    strat: Strategy,
    week: u8,
) -> Result<Lineup> {
    let mut local = local_optimize(roster, settings, strat);
    local.week = week;

    let system = format!(
        "You are an autonomous fantasy football manager. {}\n\
         You will return a starting lineup, a one-paragraph rationale, and per-slot \
         picks. Use the league settings, the current roster, and any provided news \
         strictly. Do not hallucinate players who are not on the roster.",
        strat.guidance()
    );

    let news_block = if news.is_empty() {
        String::from("(no relevant news)")
    } else {
        news.iter()
            .take(15)
            .map(|n| format!("- [{}] {}", n.source, n.title))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let roster_block = roster
        .players
        .iter()
        .map(|p| {
            format!(
                "- {} ({} {}) status={} proj={:.1} avg={:.1}",
                p.name, p.position, p.team, p.status, p.projected_points, p.avg_points
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let slot_block = settings
        .roster_slots
        .iter()
        .filter(|(p, _)| p.is_starter_slot())
        .map(|(p, c)| format!("{}x{}", p, c))
        .collect::<Vec<_>>()
        .join(", ");

    let matchup_block = matchups
        .iter()
        .find(|m| m.home_team == roster.team_name || m.away_team == roster.team_name)
        .map(|m| {
            format!(
                "Week {} matchup: {} ({:.1} proj) vs {} ({:.1} proj)",
                m.week, m.home_team, m.home_projected, m.away_team, m.away_projected
            )
        })
        .unwrap_or_else(|| format!("Week {week}"));

    let user = format!(
        "Required starting slots: {slot_block}\n\
         Scoring: {scoring}\n\
         {matchup_block}\n\n\
         === ROSTER ===\n{roster_block}\n\n\
         === RECENT NEWS ===\n{news_block}\n\n\
         === LOCAL BASELINE (greedy by risk-adjusted projection) ===\n{baseline}\n\n\
         Respond in this exact format:\n\
         REASONING: <one paragraph>\n\
         LINEUP:\n\
         <SLOT>: <Player Name>\n\
         (one line per starter slot in the order given above; bench players omitted)",
        scoring = settings.scoring,
        baseline = format_lineup(&local),
    );

    match client.complete(&system, &user).await {
        Ok(text) => Ok(parse_ai_lineup(&text, roster, settings, week, strat)
            .unwrap_or_else(|e| {
                tracing::warn!(error=%e, "failed to parse AI lineup, using local fallback");
                Lineup {
                    reasoning: format!("AI parse failed ({e}); fell back to local optimizer."),
                    ..local
                }
            })),
        Err(e) => Ok(Lineup {
            reasoning: format!("AI unavailable ({e}); using local optimizer."),
            ..local
        }),
    }
}

fn format_lineup(l: &Lineup) -> String {
    l.starters
        .iter()
        .map(|s| {
            format!(
                "{}: {}",
                s.slot,
                s.player
                    .as_ref()
                    .map(|p| p.name.as_str())
                    .unwrap_or("(empty)")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_ai_lineup(
    text: &str,
    roster: &Roster,
    settings: &LeagueSettings,
    week: u8,
    strat: Strategy,
) -> Result<Lineup> {
    let mut reasoning = String::new();
    let mut slot_assignments: Vec<(Position, String)> = Vec::new();
    let mut in_lineup = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(r) = trimmed.strip_prefix("REASONING:") {
            reasoning = r.trim().to_string();
            continue;
        }
        if trimmed.starts_with("LINEUP:") {
            in_lineup = true;
            continue;
        }
        if !in_lineup {
            if !reasoning.is_empty() {
                reasoning.push(' ');
                reasoning.push_str(trimmed);
            }
            continue;
        }
        if let Some((slot_str, name)) = trimmed.split_once(':') {
            let slot = Position::from_str(slot_str.trim());
            if slot != Position::Unknown {
                slot_assignments.push((slot, name.trim().trim_matches('*').to_string()));
            }
        }
    }

    if slot_assignments.is_empty() {
        anyhow::bail!("could not parse any slot lines from AI output");
    }

    let by_name: HashMap<String, &Player> = roster
        .players
        .iter()
        .map(|p| (p.name.to_lowercase(), p))
        .collect();

    let mut chosen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut starters = Vec::new();
    for (slot, name) in slot_assignments {
        let needle = name.to_lowercase();
        let resolved = by_name
            .get(&needle)
            .copied()
            .or_else(|| {
                roster
                    .players
                    .iter()
                    .find(|p| p.name.to_lowercase().contains(&needle) || needle.contains(&p.name.to_lowercase()))
            });
        if let Some(p) = resolved {
            if !chosen.insert(p.id.clone()) {
                continue;
            }
            starters.push(LineupSlot {
                slot,
                player: Some(p.clone()),
            });
        } else {
            starters.push(LineupSlot { slot, player: None });
        }
    }

    let projected_total: f32 = starters
        .iter()
        .filter_map(|s| s.player.as_ref().map(|p| risk_adjusted(p, strat)))
        .sum();
    let bench: Vec<Player> = roster
        .players
        .iter()
        .filter(|p| !chosen.contains(&p.id))
        .cloned()
        .collect();

    // Sanity-check: counts must match required slots, otherwise prefer local optimizer.
    let mut required: HashMap<Position, u32> = HashMap::new();
    for (p, c) in &settings.roster_slots {
        if p.is_starter_slot() {
            required.insert(*p, *c);
        }
    }
    let mut got: HashMap<Position, u32> = HashMap::new();
    for s in &starters {
        *got.entry(s.slot).or_insert(0) += 1;
    }
    if got != required {
        anyhow::bail!(
            "slot counts don't match (got {:?}, required {:?})",
            got,
            required
        );
    }

    Ok(Lineup {
        week,
        starters,
        bench,
        projected_total,
        reasoning,
    })
}
