//! Egui-based desktop GUI. Compiled when the `gui` feature is enabled.

use crate::anthropic::Anthropic;
use crate::draft::{DraftManager, DraftSuggestion};
use crate::lineup;
use crate::news::NewsFetcher;
use crate::providers::Provider;
use crate::scheduler::{AppData, Scheduler};
use crate::strategy::Strategy;
use crate::trade::{self, TradeAnalysis};
use crate::types::*;
use crate::waiver::{self, WaiverReport};
use eframe::egui;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Roster,
    Lineup,
    Waiver,
    Trade,
    Draft,
    News,
}

const ALL_TABS: &[(Tab, &str)] = &[
    (Tab::Roster, "Roster"),
    (Tab::Lineup, "Lineup"),
    (Tab::Waiver, "Waiver"),
    (Tab::Trade, "Trades"),
    (Tab::Draft, "Draft"),
    (Tab::News, "News"),
];

pub struct GuiApp {
    rt: tokio::runtime::Handle,
    provider: Arc<dyn Provider>,
    anthropic: Arc<Anthropic>,
    #[allow(dead_code)]
    news_fetcher: Arc<NewsFetcher>,
    scheduler: Arc<Scheduler>,

    strategy: Strategy,
    tab: Tab,
    status: Arc<Mutex<String>>,

    lineup: Arc<Mutex<Option<Lineup>>>,
    waiver: Arc<Mutex<Option<WaiverReport>>>,
    trade: Arc<Mutex<Option<TradeAnalysis>>>,
    draft_sugg: Arc<Mutex<Option<DraftSuggestion>>>,

    busy: Arc<Mutex<std::collections::HashSet<&'static str>>>,

    // trade UI inputs
    trade_partner: String,
    trade_send: String,
    trade_receive: String,
}

impl GuiApp {
    pub fn new(
        rt: tokio::runtime::Handle,
        provider: Arc<dyn Provider>,
        anthropic: Arc<Anthropic>,
        news_fetcher: Arc<NewsFetcher>,
        scheduler: Arc<Scheduler>,
        strategy: Strategy,
    ) -> Self {
        Self {
            rt,
            provider,
            anthropic,
            news_fetcher,
            scheduler,
            strategy,
            tab: Tab::Roster,
            status: Arc::new(Mutex::new(
                "Background refresh running. Click 'Refresh now' to update immediately."
                    .into(),
            )),
            lineup: Arc::new(Mutex::new(None)),
            waiver: Arc::new(Mutex::new(None)),
            trade: Arc::new(Mutex::new(None)),
            draft_sugg: Arc::new(Mutex::new(None)),
            busy: Arc::new(Mutex::new(Default::default())),
            trade_partner: String::new(),
            trade_send: String::new(),
            trade_receive: String::new(),
        }
    }

    fn data(&self) -> AppData {
        self.scheduler.data.read().clone()
    }

    fn set_status(&self, s: impl Into<String>) {
        *self.status.lock() = s.into();
    }

    fn is_busy(&self, key: &str) -> bool {
        self.busy.lock().contains(key)
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_secs(1));

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("groks_fantasy");
                ui.separator();
                ui.label(format!("provider: {}", self.provider.name()));
                ui.separator();
                let data = self.data();
                ui.label(format!("week: {}", data.week));
                ui.separator();
                ui.label("strategy:");
                let mut s = self.strategy;
                egui::ComboBox::from_id_salt("strategy_combo")
                    .selected_text(s.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut s, Strategy::Conservative, "Conservative");
                        ui.selectable_value(&mut s, Strategy::Balanced, "Balanced");
                        ui.selectable_value(
                            &mut s,
                            Strategy::HighStakes,
                            "High Stakes / High Rewards",
                        );
                    });
                if s != self.strategy {
                    self.strategy = s;
                    self.set_status(format!("Strategy → {}", s.label()));
                }
                ui.separator();
                if ui.button("Refresh now").clicked() {
                    self.scheduler.poke();
                    self.set_status("Refresh requested.");
                }
                ui.separator();
                if let Some(t) = data.last_refresh {
                    ui.label(format!("last refresh: {}s ago", t.elapsed().as_secs()));
                }
                if let Some(err) = &data.last_error {
                    ui.colored_label(egui::Color32::RED, format!("err: {err}"));
                }
            });
            ui.horizontal(|ui| {
                for (t, label) in ALL_TABS {
                    if ui
                        .selectable_label(self.tab == *t, *label)
                        .clicked()
                    {
                        self.tab = *t;
                    }
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(self.status.lock().clone());
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Roster => self.render_roster(ui),
            Tab::Lineup => self.render_lineup(ui, ctx),
            Tab::Waiver => self.render_waiver(ui, ctx),
            Tab::Trade => self.render_trade(ui, ctx),
            Tab::Draft => self.render_draft(ui, ctx),
            Tab::News => self.render_news(ui),
        });
    }
}

impl GuiApp {
    fn render_roster(&self, ui: &mut egui::Ui) {
        let data = self.data();
        let Some(roster) = data.roster else {
            ui.label("Waiting for first refresh…");
            return;
        };
        ui.label(format!(
            "{} ({}-{}-{}, PF {:.1}, PA {:.1})",
            roster.team_name,
            roster.wins,
            roster.losses,
            roster.ties,
            roster.points_for,
            roster.points_against
        ));
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("roster_grid")
                .num_columns(7)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Slot");
                    ui.strong("Player");
                    ui.strong("Pos");
                    ui.strong("Team");
                    ui.strong("Status");
                    ui.strong("Proj");
                    ui.strong("Avg");
                    ui.end_row();
                    for p in &roster.players {
                        let color = match p.status {
                            PlayerStatus::Out
                            | PlayerStatus::IR
                            | PlayerStatus::Suspended => egui::Color32::LIGHT_RED,
                            PlayerStatus::Doubtful => egui::Color32::YELLOW,
                            PlayerStatus::Questionable => egui::Color32::from_rgb(220, 220, 120),
                            _ => ui.style().visuals.text_color(),
                        };
                        ui.colored_label(color, p.roster_slot.to_string());
                        ui.colored_label(color, &p.name);
                        ui.colored_label(color, p.position.to_string());
                        ui.colored_label(color, &p.team);
                        ui.colored_label(color, p.status.to_string());
                        ui.colored_label(color, format!("{:.1}", p.projected_points));
                        ui.colored_label(color, format!("{:.1}", p.avg_points));
                        ui.end_row();
                    }
                });
        });
    }

    fn render_lineup(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            let busy = self.is_busy("lineup");
            if ui
                .add_enabled(!busy, egui::Button::new("Generate AI lineup"))
                .clicked()
            {
                self.spawn_lineup(ctx.clone());
            }
            if busy {
                ui.spinner();
                ui.label("contacting Claude…");
            }
        });
        ui.separator();
        let lineup = self.lineup.lock().clone();
        match lineup {
            None => {
                ui.label("No lineup yet.");
            }
            Some(l) => {
                ui.label(format!(
                    "Week {} — projected total {:.1}",
                    l.week, l.projected_total
                ));
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("lineup_grid")
                        .num_columns(4)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Slot");
                            ui.strong("Player");
                            ui.strong("Status");
                            ui.strong("Proj");
                            ui.end_row();
                            for s in &l.starters {
                                ui.label(s.slot.to_string());
                                match &s.player {
                                    Some(p) => {
                                        ui.label(&p.name);
                                        ui.label(p.status.to_string());
                                        ui.label(format!("{:.1}", p.projected_points));
                                    }
                                    None => {
                                        ui.label("(empty)");
                                        ui.label("");
                                        ui.label("");
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    ui.separator();
                    ui.heading("Reasoning");
                    ui.label(&l.reasoning);
                });
            }
        }
    }

    fn render_waiver(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            let busy = self.is_busy("waiver");
            if ui
                .add_enabled(!busy, egui::Button::new("Suggest waiver pickups"))
                .clicked()
            {
                self.spawn_waiver(ctx.clone());
            }
            if busy {
                ui.spinner();
                ui.label("scoring free agents and asking Claude…");
            }
        });
        ui.separator();
        let report = self.waiver.lock().clone();
        match report {
            None => {
                ui.label("No suggestions yet.");
            }
            Some(r) => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("waiver_grid")
                    .num_columns(7)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("#");
                        ui.strong("Pickup");
                        ui.strong("Pos/Team");
                        ui.strong("ROS");
                        ui.strong("Floor / Ceil");
                        ui.strong("Risk");
                        ui.strong("Drop");
                        ui.end_row();
                        for c in &r.candidates {
                            ui.label(c.priority.to_string());
                            ui.label(&c.player.name);
                            ui.label(format!("{} {}", c.player.position, c.player.team));
                            ui.label(format!("{:.0}", c.metrics.ros_value));
                            ui.label(format!(
                                "{:.1} / {:.1}",
                                c.metrics.floor, c.metrics.ceiling
                            ));
                            ui.label(format!("{:.2}", c.metrics.risk_score));
                            match &c.drop_candidate {
                                Some(d) => ui.label(format!(
                                    "{} (Δ{:+.0})",
                                    d.player.name, d.net_ros_delta
                                )),
                                None => ui.label("—"),
                            };
                            ui.end_row();
                        }
                    });
                ui.separator();
                ui.heading("Claude analysis");
                ui.label(&r.raw);
                });
            }
        }
    }

    fn render_trade(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Analyze a proposed trade");
        ui.label(
            "Enter players from your roster you'd send and players you'd receive. \
             Comma-separated. Partner team must match a name in your league.",
        );
        egui::Grid::new("trade_inputs")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Partner team:");
                ui.text_edit_singleline(&mut self.trade_partner);
                ui.end_row();
                ui.label("You send:");
                ui.text_edit_singleline(&mut self.trade_send);
                ui.end_row();
                ui.label("You receive:");
                ui.text_edit_singleline(&mut self.trade_receive);
                ui.end_row();
            });
        let busy = self.is_busy("trade");
        if ui
            .add_enabled(!busy, egui::Button::new("Analyze trade"))
            .clicked()
        {
            self.spawn_trade(ctx.clone());
        }
        if busy {
            ui.spinner();
            ui.label("computing metrics and asking Claude…");
        }
        ui.separator();
        let analysis = self.trade.lock().clone();
        if let Some(a) = analysis {
            let verdict_color = match a.verdict {
                "ACCEPT" => egui::Color32::LIGHT_GREEN,
                "DECLINE" => egui::Color32::LIGHT_RED,
                _ => egui::Color32::YELLOW,
            };
            ui.horizontal(|ui| {
                ui.heading("Verdict:");
                ui.colored_label(verdict_color, a.verdict);
                ui.label(format!(
                    " | net ROS {:+.1} | fairness {:.0}%",
                    a.net_ros_delta,
                    a.fairness * 100.0
                ));
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("You send");
                for m in &a.send {
                    ui.label(m.one_line());
                }
                ui.label(format!(
                    "  package — mean {:.1}, ceiling {:.1}, ROS {:.0}, avg risk {:.2}",
                    a.send_pkg.total_mean,
                    a.send_pkg.total_ceiling,
                    a.send_pkg.total_ros,
                    a.send_pkg.avg_risk
                ));
                ui.separator();
                ui.heading("You receive");
                for m in &a.receive {
                    ui.label(m.one_line());
                }
                ui.label(format!(
                    "  package — mean {:.1}, ceiling {:.1}, ROS {:.0}, avg risk {:.2}",
                    a.receive_pkg.total_mean,
                    a.receive_pkg.total_ceiling,
                    a.receive_pkg.total_ros,
                    a.receive_pkg.avg_risk
                ));
                ui.separator();
                ui.heading("Claude analysis");
                ui.label(&a.ai_summary);
            });
        }
    }

    fn render_draft(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            let busy = self.is_busy("draft");
            if ui
                .add_enabled(!busy, egui::Button::new("Refresh draft & suggest"))
                .clicked()
            {
                self.spawn_draft(ctx.clone());
            }
            if busy {
                ui.spinner();
                ui.label("asking Claude for picks…");
            }
        });
        ui.separator();
        let data = self.data();
        if let Some(d) = &data.draft {
            ui.label(format!(
                "Pick #{} — round {} of {} — {} teams — completed: {}",
                d.current_pick,
                ((d.current_pick.saturating_sub(1) / d.team_count.max(1)) + 1),
                d.total_rounds,
                d.team_count,
                d.completed
            ));
            ui.separator();
            ui.heading("Recent picks");
            for p in d.picks.iter().rev().take(10).rev() {
                ui.label(format!(
                    "R{}.{} {} → {}",
                    p.round,
                    p.pick_number,
                    p.team_name,
                    p.player_name.as_deref().unwrap_or("?")
                ));
            }
            ui.separator();
        } else {
            ui.label("No draft state available.");
        }
        if let Some(s) = self.draft_sugg.lock().clone() {
            ui.heading("Top 3 candidates");
            for p in &s.picks {
                ui.label(format!(
                    "{}. {} ({}) — {}",
                    p.rank,
                    p.name,
                    p.position.map(|x| x.to_string()).unwrap_or_else(|| "?".into()),
                    p.rationale
                ));
            }
        }
    }

    fn render_news(&self, ui: &mut egui::Ui) {
        let data = self.data();
        if data.news.is_empty() {
            ui.label("No news ingested yet.");
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for n in data.news.iter().take(80) {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("[{}]", n.source)).strong());
                    if !n.url.is_empty() {
                        ui.hyperlink_to(&n.title, &n.url);
                    } else {
                        ui.label(&n.title);
                    }
                });
            }
        });
    }

    fn spawn_lineup(&self, ctx: egui::Context) {
        let key = "lineup";
        self.busy.lock().insert(key);
        let data = self.data();
        let Some(roster) = data.roster else {
            self.set_status("No roster — wait for refresh.");
            self.busy.lock().remove(key);
            return;
        };
        let settings = data.settings.unwrap_or_default();
        let week = data.week;
        let news = data.news;
        let matchups = data.matchups;
        let strat = self.strategy;
        let anthropic = self.anthropic.clone();
        let lineup_slot = self.lineup.clone();
        let busy = self.busy.clone();
        let status = self.status.clone();
        self.rt.spawn(async move {
            let res = lineup::ai_optimize(
                &anthropic, &roster, &settings, &matchups, &news, strat, week,
            )
            .await;
            match res {
                Ok(l) => {
                    *status.lock() = format!(
                        "Lineup ready — projected {:.1} pts ({})",
                        l.projected_total,
                        strat.label()
                    );
                    *lineup_slot.lock() = Some(l);
                }
                Err(e) => *status.lock() = format!("lineup error: {e}"),
            }
            busy.lock().remove(key);
            ctx.request_repaint();
        });
    }

    fn spawn_waiver(&self, ctx: egui::Context) {
        let key = "waiver";
        self.busy.lock().insert(key);
        let provider = self.provider.clone();
        let anthropic = self.anthropic.clone();
        let strat = self.strategy;
        let news = self.data().news;
        let waiver_slot = self.waiver.clone();
        let busy = self.busy.clone();
        let status = self.status.clone();
        self.rt.spawn(async move {
            let res = waiver::analyze(provider.as_ref(), &anthropic, strat, &news, 200).await;
            match res {
                Ok(r) => {
                    *status.lock() = format!("Waiver suggestions ready ({} candidates)", r.candidates.len());
                    *waiver_slot.lock() = Some(r);
                }
                Err(e) => *status.lock() = format!("waiver error: {e}"),
            }
            busy.lock().remove(key);
            ctx.request_repaint();
        });
    }

    fn spawn_trade(&self, ctx: egui::Context) {
        let key = "trade";
        let send_input = self.trade_send.clone();
        let recv_input = self.trade_receive.clone();
        let partner = self.trade_partner.clone();
        if send_input.trim().is_empty() || recv_input.trim().is_empty() || partner.trim().is_empty()
        {
            self.set_status("Fill in partner, send, and receive.");
            return;
        }
        self.busy.lock().insert(key);
        let data = self.data();
        let Some(my_roster) = data.roster.clone() else {
            self.set_status("No roster — wait for refresh.");
            self.busy.lock().remove(key);
            return;
        };
        let other_rosters = data.all_rosters.clone();
        let news = data.news.clone();
        let strat = self.strategy;
        let anthropic = self.anthropic.clone();
        let trade_slot = self.trade.clone();
        let busy = self.busy.clone();
        let status = self.status.clone();
        self.rt.spawn(async move {
            let send: Vec<String> = send_input
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let recv: Vec<String> = recv_input
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let res = trade::analyze(
                &anthropic,
                &my_roster,
                &partner,
                &send,
                &recv,
                &other_rosters,
                strat,
                &news,
            )
            .await;
            match res {
                Ok(a) => {
                    *status.lock() = format!(
                        "Trade verdict: {} (net ROS {:+.1})",
                        a.verdict, a.net_ros_delta
                    );
                    *trade_slot.lock() = Some(a);
                }
                Err(e) => *status.lock() = format!("trade error: {e}"),
            }
            busy.lock().remove(key);
            ctx.request_repaint();
        });
    }

    fn spawn_draft(&self, ctx: egui::Context) {
        let key = "draft";
        self.busy.lock().insert(key);
        let provider = self.provider.clone();
        let anthropic = self.anthropic.clone();
        let news = self.data().news;
        let team_name = self
            .data()
            .roster
            .map(|r| r.team_name)
            .unwrap_or_default();
        let strat = self.strategy;
        let sugg_slot = self.draft_sugg.clone();
        let busy = self.busy.clone();
        let status = self.status.clone();
        self.rt.spawn(async move {
            let dm = DraftManager {
                provider: provider.as_ref(),
                anthropic: &anthropic,
                strategy: strat,
                my_team_name: team_name,
                poll_interval: Duration::from_secs(5),
            };
            let snapshot = match dm.snapshot().await {
                Ok(s) => s,
                Err(e) => {
                    *status.lock() = format!("draft snapshot error: {e}");
                    busy.lock().remove(key);
                    ctx.request_repaint();
                    return;
                }
            };
            let my_picks: Vec<DraftPick> = snapshot
                .picks
                .iter()
                .filter(|p| p.team_name.eq_ignore_ascii_case(&dm.my_team_name))
                .cloned()
                .collect();
            match dm.ask_claude(&snapshot, &my_picks, &[], &news).await {
                Ok(s) => {
                    *status.lock() = format!("Draft suggestions ready ({} candidates)", s.picks.len());
                    *sugg_slot.lock() = Some(s);
                }
                Err(e) => *status.lock() = format!("draft AI error: {e}"),
            }
            busy.lock().remove(key);
            ctx.request_repaint();
        });
    }
}

pub fn run(
    rt: tokio::runtime::Handle,
    provider: Arc<dyn Provider>,
    anthropic: Anthropic,
    news_fetcher: Arc<NewsFetcher>,
    scheduler: Arc<Scheduler>,
    strategy: Strategy,
) -> anyhow::Result<()> {
    let app = GuiApp::new(
        rt,
        provider,
        Arc::new(anthropic),
        news_fetcher,
        scheduler,
        strategy,
    );
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_title("groks_fantasy — AI fantasy football manager"),
        ..Default::default()
    };
    eframe::run_native(
        "groks_fantasy",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))
}
