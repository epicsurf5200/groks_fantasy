use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NewsItem {
    pub title: String,
    pub summary: String,
    pub source: String,
    pub url: String,
    pub published: String,
}

#[derive(Debug, Deserialize)]
struct Rss {
    channel: Channel,
}

#[derive(Debug, Deserialize)]
struct Channel {
    #[serde(default, rename = "item")]
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Item {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    link: String,
    #[serde(default, rename = "pubDate")]
    pub_date: String,
}

pub struct NewsFetcher {
    http: Client,
    sources: Vec<String>,
}

impl NewsFetcher {
    pub fn new(sources: Vec<String>) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("Mozilla/5.0 groks_fantasy/0.2")
            .build()?;
        Ok(Self { http, sources })
    }

    pub async fn fetch_all(&self, limit: usize) -> Vec<NewsItem> {
        let mut all = Vec::new();
        for src in &self.sources {
            match self.fetch(src).await {
                Ok(items) => all.extend(items),
                Err(e) => tracing::warn!(source=%src, err=%e, "news source failed"),
            }
        }
        all.truncate(limit);
        all
    }

    async fn fetch(&self, url: &str) -> Result<Vec<NewsItem>> {
        let body = self.http.get(url).send().await?.error_for_status()?.text().await?;
        let rss: Rss = quick_xml::de::from_str(&body)?;
        let host = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "rss".into());
        Ok(rss
            .channel
            .items
            .into_iter()
            .map(|it| NewsItem {
                title: strip_html(&it.title),
                summary: strip_html(&it.description),
                source: host.clone(),
                url: it.link,
                published: it.pub_date,
            })
            .collect())
    }
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Filter news to items that mention any roster player by name.
pub fn relevant_to(items: &[NewsItem], player_names: &[String]) -> Vec<NewsItem> {
    items
        .iter()
        .filter(|n| {
            let blob = format!("{} {}", n.title, n.summary).to_lowercase();
            player_names
                .iter()
                .any(|p| !p.is_empty() && blob.contains(&p.to_lowercase()))
        })
        .cloned()
        .collect()
}
