use crate::anthropic::Anthropic;
use crate::draft::DraftManager;
use crate::lineup;
use crate::news::{self, NewsFetcher, NewsItem};
use crate::providers::Provider;
use crate::strategy::Strategy;
use crate::types::*;
use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap};
use ratatui::Terminal;
use std::io;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Roster,
    Lineup,
    Draft,
    News,
    Help,
}

impl Tab {
    const ALL: [Tab; 5] = [Tab::Roster, Tab::Lineup, Tab::Draft, Tab::News, Tab::Help];
    fn label(&self) -> &'static str {
        match self {
            Self::Roster => "Roster",
            Self::Lineup => "Lineup",
            Self::Draft => "Draft",
            Self::News => "News",
            Self::Help => "Help",
        }
    }
}

pub struct App {
    pub provider: Box<dyn Provider>,
    pub anthropic: Anthropic,
    pub news_fetcher: NewsFetcher,
    pub strategy: Strategy,
    tab: Tab,
    status: String,
    roster: Option<Roster>,
    settings: Option<LeagueSettings>,
    week: u8,
    matchups: Vec<Matchup>,
    news: Vec<NewsItem>,
    lineup: Option<Lineup>,
    draft: Option<crate::types::DraftState>,
    draft_suggestion: Option<crate::draft::DraftSuggestion>,
    last_refresh: Instant,
}

impl App {
    pub fn new(
        provider: Box<dyn Provider>,
        anthropic: Anthropic,
        news_fetcher: NewsFetcher,
        strategy: Strategy,
    ) -> Self {
        Self {
            provider,
            anthropic,
            news_fetcher,
            strategy,
            tab: Tab::Roster,
            status: "Press 'r' to refresh, 'l' for AI lineup, 'd' for draft suggestion, 'q' to quit".into(),
            roster: None,
            settings: None,
            week: 1,
            matchups: Vec::new(),
            news: Vec::new(),
            lineup: None,
            draft: None,
            draft_suggestion: None,
            last_refresh: Instant::now() - Duration::from_secs(3600),
        }
    }

    pub async fn run(mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let res = self.event_loop(&mut terminal).await;

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        res
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        // Initial refresh
        self.refresh().await;

        loop {
            terminal.draw(|f| self.render(f))?;

            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Tab | KeyCode::Right => self.cycle_tab(1),
                        KeyCode::BackTab | KeyCode::Left => self.cycle_tab(-1),
                        KeyCode::Char('1') => self.tab = Tab::Roster,
                        KeyCode::Char('2') => self.tab = Tab::Lineup,
                        KeyCode::Char('3') => self.tab = Tab::Draft,
                        KeyCode::Char('4') => self.tab = Tab::News,
                        KeyCode::Char('?') | KeyCode::Char('5') => self.tab = Tab::Help,
                        KeyCode::Char('r') => self.refresh().await,
                        KeyCode::Char('l') => self.compute_lineup().await,
                        KeyCode::Char('d') => self.compute_draft().await,
                        KeyCode::Char('s') => self.cycle_strategy(),
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn cycle_tab(&mut self, dir: i32) {
        let mut idx = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0) as i32;
        idx = (idx + dir).rem_euclid(Tab::ALL.len() as i32);
        self.tab = Tab::ALL[idx as usize];
    }

    fn cycle_strategy(&mut self) {
        self.strategy = match self.strategy {
            Strategy::Conservative => Strategy::Balanced,
            Strategy::Balanced => Strategy::HighStakes,
            Strategy::HighStakes => Strategy::Conservative,
        };
        self.status = format!("Strategy → {}", self.strategy.label());
    }

    async fn refresh(&mut self) {
        self.status = "Refreshing roster, matchups, news…".into();
        match self.provider.league_settings().await {
            Ok(s) => self.settings = Some(s),
            Err(e) => self.status = format!("settings error: {e}"),
        }
        match self.provider.current_week().await {
            Ok(w) => self.week = w,
            Err(e) => self.status = format!("week error: {e}"),
        }
        match self.provider.my_roster().await {
            Ok(r) => self.roster = Some(r),
            Err(e) => self.status = format!("roster error: {e}"),
        }
        match self.provider.matchups(self.week).await {
            Ok(m) => self.matchups = m,
            Err(e) => self.status = format!("matchups error: {e}"),
        }
        let mut news = self.news_fetcher.fetch_all(50).await;
        if let Some(roster) = &self.roster {
            let names: Vec<String> = roster.players.iter().map(|p| p.name.clone()).collect();
            let filtered = news::relevant_to(&news, &names);
            if !filtered.is_empty() {
                news = filtered;
            }
        }
        self.news = news;
        self.last_refresh = Instant::now();
        self.status = format!(
            "Refreshed at {} (provider={}, week={}, strategy={})",
            chrono::Local::now().format("%H:%M:%S"),
            self.provider.name(),
            self.week,
            self.strategy.label()
        );
    }

    async fn compute_lineup(&mut self) {
        let Some(roster) = self.roster.clone() else {
            self.status = "Refresh first (r)".into();
            return;
        };
        let settings = self.settings.clone().unwrap_or_default();
        self.status = format!("Asking Claude ({}) for {} lineup…", self.anthropic.model(), self.strategy.label());
        match lineup::ai_optimize(
            &self.anthropic,
            &roster,
            &settings,
            &self.matchups,
            &self.news,
            self.strategy,
            self.week,
        )
        .await
        {
            Ok(l) => {
                self.status = format!(
                    "Lineup ready — projected {:.1} pts ({} strategy)",
                    l.projected_total,
                    self.strategy.label()
                );
                self.lineup = Some(l);
                self.tab = Tab::Lineup;
            }
            Err(e) => self.status = format!("lineup error: {e}"),
        }
    }

    async fn compute_draft(&mut self) {
        self.status = "Fetching draft state…".into();
        let team_name = self
            .roster
            .as_ref()
            .map(|r| r.team_name.clone())
            .unwrap_or_default();
        let dm = DraftManager {
            provider: self.provider.as_ref(),
            anthropic: &self.anthropic,
            strategy: self.strategy,
            my_team_name: team_name,
            poll_interval: Duration::from_secs(2),
        };
        match dm.snapshot().await {
            Ok(state) => {
                self.draft = Some(state.clone());
                let my_picks: Vec<DraftPick> = state
                    .picks
                    .iter()
                    .filter(|p| {
                        self.roster
                            .as_ref()
                            .map(|r| r.team_name.eq_ignore_ascii_case(&p.team_name))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();
                self.status = "Asking Claude for next picks…".into();
                match dm.ask_claude(&state, &my_picks, &[], &self.news).await {
                    Ok(s) => {
                        self.status = format!("Draft suggestions ready ({} candidates)", s.picks.len());
                        self.draft_suggestion = Some(s);
                        self.tab = Tab::Draft;
                    }
                    Err(e) => self.status = format!("draft AI error: {e}"),
                }
            }
            Err(e) => self.status = format!("draft state error: {e}"),
        }
    }

    fn render(&self, f: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(f.area());

        self.render_tabs(f, chunks[0]);
        match self.tab {
            Tab::Roster => self.render_roster(f, chunks[1]),
            Tab::Lineup => self.render_lineup(f, chunks[1]),
            Tab::Draft => self.render_draft(f, chunks[1]),
            Tab::News => self.render_news(f, chunks[1]),
            Tab::Help => self.render_help(f, chunks[1]),
        }
        self.render_status(f, chunks[2]);
    }

    fn render_tabs(&self, f: &mut ratatui::Frame, area: Rect) {
        let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.label())).collect();
        let selected = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        let tabs = Tabs::new(titles)
            .select(selected)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(
                        " groks_fantasy — {} | week {} | strategy {} ",
                        self.provider.name(),
                        self.week,
                        self.strategy.label()
                    )),
            )
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        f.render_widget(tabs, area);
    }

    fn render_status(&self, f: &mut ratatui::Frame, area: Rect) {
        let p = Paragraph::new(self.status.clone())
            .block(Block::default().borders(Borders::ALL).title(" Status "))
            .wrap(Wrap { trim: true });
        f.render_widget(p, area);
    }

    fn render_roster(&self, f: &mut ratatui::Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" My Roster ");
        let items: Vec<ListItem> = match &self.roster {
            None => vec![ListItem::new("No data — press 'r' to refresh.")],
            Some(r) => r
                .players
                .iter()
                .map(|p| {
                    let line = format!(
                        "{slot:>5}  {name:<24}  {pos:>3}  {team:<4}  {status:>4}  proj {proj:>5.1}  avg {avg:>5.1}",
                        slot = p.roster_slot.to_string(),
                        name = truncate(&p.name, 24),
                        pos = p.position.to_string(),
                        team = p.team,
                        status = p.status.to_string(),
                        proj = p.projected_points,
                        avg = p.avg_points,
                    );
                    let style = match p.status {
                        PlayerStatus::Out | PlayerStatus::IR | PlayerStatus::Suspended => {
                            Style::default().fg(Color::Red)
                        }
                        PlayerStatus::Doubtful => Style::default().fg(Color::Yellow),
                        PlayerStatus::Questionable => Style::default().fg(Color::LightYellow),
                        _ => Style::default(),
                    };
                    ListItem::new(Line::from(Span::styled(line, style)))
                })
                .collect(),
        };
        f.render_widget(List::new(items).block(block), area);
    }

    fn render_lineup(&self, f: &mut ratatui::Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(8)])
            .split(area);
        let items: Vec<ListItem> = match &self.lineup {
            None => vec![ListItem::new(
                "Press 'l' for an AI-generated lineup using your current strategy.",
            )],
            Some(l) => l
                .starters
                .iter()
                .map(|s| {
                    let name = s
                        .player
                        .as_ref()
                        .map(|p| {
                            format!(
                                "{} ({} {}, {}) proj {:.1}",
                                p.name, p.position, p.team, p.status, p.projected_points
                            )
                        })
                        .unwrap_or_else(|| "(no eligible player)".into());
                    ListItem::new(format!("{:<6}  {}", s.slot.to_string(), name))
                })
                .collect(),
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Recommended Lineup — week {} ", self.week));
        f.render_widget(List::new(items).block(block), chunks[0]);

        let reasoning = self
            .lineup
            .as_ref()
            .map(|l| l.reasoning.clone())
            .unwrap_or_else(|| "—".into());
        let proj = self
            .lineup
            .as_ref()
            .map(|l| format!("Projected total: {:.1}\n\n{}", l.projected_total, reasoning))
            .unwrap_or_else(|| reasoning.clone());
        f.render_widget(
            Paragraph::new(proj)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title(" Reasoning ")),
            chunks[1],
        );
    }

    fn render_draft(&self, f: &mut ratatui::Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(7), Constraint::Min(1)])
            .split(area);

        let header = match &self.draft {
            None => "Press 'd' to fetch draft state and AI suggestions.".to_string(),
            Some(d) => format!(
                "Pick #{} (round {}/{}) — {} teams — completed: {}\nLast 5 picks:\n{}",
                d.current_pick,
                ((d.current_pick.saturating_sub(1) / d.team_count.max(1)) + 1),
                d.total_rounds,
                d.team_count,
                d.completed,
                d.picks
                    .iter()
                    .rev()
                    .take(5)
                    .rev()
                    .map(|p| format!(
                        "  R{}.{} {} → {}",
                        p.round,
                        p.pick_number,
                        p.team_name,
                        p.player_name.as_deref().unwrap_or("?")
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        };
        f.render_widget(
            Paragraph::new(header)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title(" Draft ")),
            chunks[0],
        );

        let items: Vec<ListItem> = match &self.draft_suggestion {
            None => vec![ListItem::new("(no suggestion yet)")],
            Some(s) => s
                .picks
                .iter()
                .map(|p| {
                    let pos = p.position.map(|x| x.to_string()).unwrap_or_else(|| "?".into());
                    ListItem::new(format!(
                        "{}. {} ({}) — {}",
                        p.rank, p.name, pos, p.rationale
                    ))
                })
                .collect(),
        };
        f.render_widget(
            List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Top 3 candidates "),
            ),
            chunks[1],
        );
    }

    fn render_news(&self, f: &mut ratatui::Frame, area: Rect) {
        let items: Vec<ListItem> = if self.news.is_empty() {
            vec![ListItem::new("No news fetched yet — press 'r'.")]
        } else {
            self.news
                .iter()
                .take(50)
                .map(|n| {
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("[{}] ", n.source), Style::default().fg(Color::Cyan)),
                        Span::raw(n.title.clone()),
                    ]))
                })
                .collect()
        };
        f.render_widget(
            List::new(items).block(Block::default().borders(Borders::ALL).title(" News ")),
            area,
        );
    }

    fn render_help(&self, f: &mut ratatui::Frame, area: Rect) {
        let body = "\
Keys
  r        Refresh roster, matchups, news
  l        Generate AI lineup for current week
  d        Fetch draft state and AI pick suggestions
  s        Cycle strategy (Conservative → Balanced → High Stakes)
  Tab/← →  Switch tab
  1-5      Jump to tab
  q / Esc  Quit

Strategies
  Conservative — floor over ceiling, healthy starters, avoid Q tags.
  Balanced     — risk-adjusted projections (default).
  High Stakes  — maximize ceiling, lean into shootouts and breakouts.

Providers
  ESPN     — set espn.{league_id, season, team_id, swid, espn_s2}
  Sleeper  — set sleeper.{league_id, user_id|username}
  Yahoo    — set yahoo.{league_key, team_key, access_token}

API
  Claude (Anthropic) is the only LLM provider. Set ANTHROPIC_API_KEY
  or anthropic.api_key in config.yaml.";
        f.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL).title(" Help ")),
            area,
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
