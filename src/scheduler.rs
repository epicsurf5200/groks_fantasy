//! Background refresh loop. One snapshot struct (`AppData`) feeds every UI.
//! Compared to the ESPN-era version, the Sleeper snapshot also carries
//! transactions, trending adds/drops, traded picks, and playoff brackets.

use crate::api::LeagueSession;
use crate::news::{self, NewsFetcher, NewsItem};
use crate::types::*;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

#[derive(Default, Clone)]
pub struct AppData {
    pub roster: Option<Roster>,
    pub all_rosters: Vec<Roster>,
    pub settings: Option<LeagueSettings>,
    pub week: u8,
    pub matchups: Vec<Matchup>,
    pub news: Vec<NewsItem>,
    pub draft: Option<DraftState>,
    pub transactions: Vec<Transaction>,
    pub trending_add: Vec<TrendingPlayer>,
    pub trending_drop: Vec<TrendingPlayer>,
    pub traded_picks: Vec<TradedPick>,
    pub winners_bracket: Vec<BracketMatch>,
    pub losers_bracket: Vec<BracketMatch>,
    pub last_refresh: Option<Instant>,
    pub last_error: Option<String>,
}

pub struct Scheduler {
    pub data: Arc<RwLock<AppData>>,
    pub notify: Arc<Notify>,
    pub interval: Duration,
}

impl Scheduler {
    pub fn new(interval: Duration) -> Self {
        Self {
            data: Arc::new(RwLock::new(AppData::default())),
            notify: Arc::new(Notify::new()),
            interval,
        }
    }

    pub fn poke(&self) {
        self.notify.notify_one();
    }

    pub fn spawn(
        &self,
        session: Arc<LeagueSession>,
        news_fetcher: Arc<NewsFetcher>,
    ) -> tokio::task::JoinHandle<()> {
        let data = self.data.clone();
        let notify = self.notify.clone();
        let interval = self.interval;
        tokio::spawn(async move {
            loop {
                if let Err(e) = refresh_once(&session, &news_fetcher, &data).await {
                    data.write().last_error = Some(e.to_string());
                }
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {}
                    _ = notify.notified() => {}
                }
            }
        })
    }
}

pub async fn refresh_once(
    session: &LeagueSession,
    news_fetcher: &NewsFetcher,
    data: &Arc<RwLock<AppData>>,
) -> anyhow::Result<()> {
    let week = session.current_week().await.unwrap_or(1);
    let settings = session.league_settings().await.ok();
    let all_rosters = session.all_rosters(week).await.unwrap_or_default();
    let roster = session.my_roster(week).await.ok();
    let matchups = session.matchups(week).await.unwrap_or_default();
    let transactions = session.recent_transactions(week, 3).await.unwrap_or_default();
    let trending_add = session
        .trending_players(TrendDirection::Add, 25)
        .await
        .unwrap_or_default();
    let trending_drop = session
        .trending_players(TrendDirection::Drop, 25)
        .await
        .unwrap_or_default();
    let traded_picks = session.traded_picks().await.unwrap_or_default();
    let (winners_bracket, losers_bracket) =
        session.playoff_bracket().await.unwrap_or_default();
    let draft = session.draft_state().await.ok();

    let mut feed = news_fetcher.fetch_all(80).await;
    if let Some(r) = &roster {
        let names: Vec<String> = r.players.iter().map(|p| p.name.clone()).collect();
        let filtered = news::relevant_to(&feed, &names);
        if !filtered.is_empty() {
            feed = filtered;
        }
    }

    let mut g = data.write();
    g.week = week;
    g.settings = settings;
    g.roster = roster;
    g.all_rosters = all_rosters;
    g.matchups = matchups;
    g.news = feed;
    g.draft = draft;
    g.transactions = transactions;
    g.trending_add = trending_add;
    g.trending_drop = trending_drop;
    g.traded_picks = traded_picks;
    g.winners_bracket = winners_bracket;
    g.losers_bracket = losers_bracket;
    g.last_refresh = Some(Instant::now());
    g.last_error = None;
    Ok(())
}
