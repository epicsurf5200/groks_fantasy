use crate::news::{self, NewsFetcher, NewsItem};
use crate::providers::Provider;
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

    /// Trigger an immediate refresh on top of the regular cadence.
    pub fn poke(&self) {
        self.notify.notify_one();
    }

    /// Spawn the background refresh loop. Returns immediately.
    pub fn spawn(
        &self,
        provider: Arc<dyn Provider>,
        news_fetcher: Arc<NewsFetcher>,
    ) -> tokio::task::JoinHandle<()> {
        let data = self.data.clone();
        let notify = self.notify.clone();
        let interval = self.interval;
        tokio::spawn(async move {
            loop {
                if let Err(e) = refresh_once(provider.as_ref(), news_fetcher.as_ref(), &data).await
                {
                    let mut g = data.write();
                    g.last_error = Some(e.to_string());
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
    provider: &dyn Provider,
    news_fetcher: &NewsFetcher,
    data: &Arc<RwLock<AppData>>,
) -> anyhow::Result<()> {
    let settings = provider.league_settings().await.ok();
    let week = provider.current_week().await.unwrap_or(1);
    let roster = provider.my_roster().await.ok();
    let all_rosters = provider.all_rosters().await.unwrap_or_default();
    let matchups = provider.matchups(week).await.unwrap_or_default();
    let mut news = news_fetcher.fetch_all(80).await;
    if let Some(r) = &roster {
        let names: Vec<String> = r.players.iter().map(|p| p.name.clone()).collect();
        let filtered = news::relevant_to(&news, &names);
        if !filtered.is_empty() {
            news = filtered;
        }
    }
    let draft = provider.draft_state().await.ok();
    let mut g = data.write();
    g.roster = roster;
    g.all_rosters = all_rosters;
    g.settings = settings;
    g.week = week;
    g.matchups = matchups;
    g.news = news;
    g.draft = draft;
    g.last_refresh = Some(Instant::now());
    g.last_error = None;
    Ok(())
}
