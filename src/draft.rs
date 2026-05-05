use crate::anthropic::Anthropic;
use crate::news::NewsItem;
use crate::providers::Provider;
use crate::strategy::Strategy;
use crate::types::*;
use anyhow::Result;
use std::collections::HashSet;
use std::time::Duration;

pub struct DraftManager<'a> {
    pub provider: &'a dyn Provider,
    pub anthropic: &'a Anthropic,
    pub strategy: Strategy,
    pub my_team_name: String,
    #[allow(dead_code)]
    pub poll_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct DraftSuggestion {
    pub picks: Vec<SuggestedPick>,
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct SuggestedPick {
    pub rank: u32,
    pub name: String,
    pub position: Option<Position>,
    pub rationale: String,
}

#[allow(dead_code)]
impl<'a> DraftManager<'a> {
    pub async fn snapshot(&self) -> Result<DraftState> {
        self.provider.draft_state().await
    }

    /// Returns true when the next pick belongs to our team.
    pub async fn is_my_turn(&self, state: &DraftState) -> bool {
        state
            .on_the_clock_team
            .as_deref()
            .map(|t| t.eq_ignore_ascii_case(&self.my_team_name))
            .unwrap_or(false)
    }

    pub async fn ask_claude(
        &self,
        state: &DraftState,
        my_picks: &[DraftPick],
        available_pool_hint: &[String],
        news: &[NewsItem],
    ) -> Result<DraftSuggestion> {
        let system = format!(
            "You are an autonomous fantasy football draft assistant. {}\n\
             Recommend exactly 3 players for the next pick. Respect positional needs, \
             bye-week distribution, handcuffs/sleepers, and the strategy guidance above. \
             Do NOT recommend any player that has already been drafted.",
            self.strategy.guidance()
        );

        let drafted: HashSet<String> = state
            .picks
            .iter()
            .filter_map(|p| p.player_name.as_ref().map(|n| n.to_lowercase()))
            .collect();

        let recent_picks = state
            .picks
            .iter()
            .rev()
            .take(20)
            .rev()
            .map(|p| {
                format!(
                    "  R{}.{} {} -> {}",
                    p.round,
                    p.pick_number,
                    p.team_name,
                    p.player_name.as_deref().unwrap_or("?"),
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let my_block = if my_picks.is_empty() {
            "(no picks yet)".to_string()
        } else {
            my_picks
                .iter()
                .map(|p| {
                    format!(
                        "  R{}.{} {}",
                        p.round,
                        p.pick_number,
                        p.player_name.as_deref().unwrap_or("?")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let news_block = if news.is_empty() {
            "(no recent news provided)".to_string()
        } else {
            news.iter()
                .take(10)
                .map(|n| format!("- [{}] {}", n.source, n.title))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let pool_block = if available_pool_hint.is_empty() {
            "(use your own ranking knowledge)".to_string()
        } else {
            let filtered: Vec<&String> = available_pool_hint
                .iter()
                .filter(|n| !drafted.contains(&n.to_lowercase()))
                .take(50)
                .collect();
            filtered
                .iter()
                .enumerate()
                .map(|(i, n)| format!("  {}. {}", i + 1, n))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let user = format!(
            "Current pick: overall #{} (round {} of {}).\n\
             Total teams: {}.\n\
             My team: {}\n\
             My picks so far:\n{}\n\n\
             Recent picks (last 20):\n{}\n\n\
             Available pool hints:\n{}\n\n\
             Recent news:\n{}\n\n\
             Respond in this exact format (3 entries):\n\
             1. <Name> (<POS>) — <one-line rationale>\n\
             2. <Name> (<POS>) — <one-line rationale>\n\
             3. <Name> (<POS>) — <one-line rationale>",
            state.current_pick,
            ((state.current_pick.saturating_sub(1) / state.team_count.max(1)) + 1),
            state.total_rounds,
            state.team_count,
            self.my_team_name,
            my_block,
            recent_picks,
            pool_block,
            news_block,
        );

        let raw = self.anthropic.complete(&system, &user).await?;
        let picks = parse_suggestions(&raw);
        Ok(DraftSuggestion { picks, raw })
    }
}

fn parse_suggestions(text: &str) -> Vec<SuggestedPick> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed
            .strip_prefix("1.")
            .or_else(|| trimmed.strip_prefix("2."))
            .or_else(|| trimmed.strip_prefix("3."))
        else {
            continue;
        };
        let rank = trimmed.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(0);
        let rest = rest.trim();
        let (name_part, rationale) = match rest.split_once('—').or_else(|| rest.split_once('-')) {
            Some((n, r)) => (n.trim().to_string(), r.trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
        let (name, position) = match name_part.rsplit_once('(') {
            Some((n, p)) => {
                let p = p.trim_end_matches(')').trim();
                let pos = Position::from_str(p);
                (n.trim().to_string(), if pos == Position::Unknown { None } else { Some(pos) })
            }
            None => (name_part, None),
        };
        out.push(SuggestedPick {
            rank,
            name,
            position,
            rationale,
        });
    }
    out
}
