use crate::types::*;
use anyhow::{anyhow, Result};
use async_trait::async_trait;

pub mod espn;
pub mod sleeper;
pub mod yahoo;

#[async_trait]
#[allow(dead_code)]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn league_settings(&self) -> Result<LeagueSettings>;
    async fn current_week(&self) -> Result<u8>;

    async fn my_roster(&self) -> Result<Roster>;
    async fn all_rosters(&self) -> Result<Vec<Roster>>;

    async fn matchups(&self, week: u8) -> Result<Vec<Matchup>>;
    async fn free_agents(&self, position: Option<Position>, limit: usize) -> Result<Vec<Player>>;

    async fn draft_state(&self) -> Result<DraftState>;

    async fn set_lineup(&self, _lineup: &Lineup) -> Result<()> {
        Err(anyhow!(
            "automatic lineup setting is not yet supported for this provider — apply the recommendation manually"
        ))
    }
}

pub fn build(cfg: &crate::config::ProviderConfig) -> Result<Box<dyn Provider>> {
    match cfg {
        crate::config::ProviderConfig::Espn(c) => Ok(Box::new(espn::EspnProvider::new(c.clone())?)),
        crate::config::ProviderConfig::Sleeper(c) => {
            Ok(Box::new(sleeper::SleeperProvider::new(c.clone())?))
        }
        crate::config::ProviderConfig::Yahoo(c) => {
            Ok(Box::new(yahoo::YahooProvider::new(c.clone())?))
        }
    }
}
