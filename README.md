# groks_fantasy

An autonomous, **Claude-powered** fantasy-football manager. Pulls your roster,
matchups, projections, and live news; recommends or sets a starting lineup
according to a user-chosen strategy; suggests waiver pickups, evaluates trades
with quantified metrics, and runs an interactive draft assistant.

> Despite the name, the LLM provider is **Anthropic Claude** — not Grok.

## Features

- **Multi-provider:** ESPN, Sleeper (no-auth public API), and Yahoo (OAuth2 token).
- **Strategy modes:** `conservative` / `balanced` / `high_stakes` (each with
  prompt guidance and risk multipliers used in metric calculation).
- **AI lineup optimizer** — Claude refines a deterministic greedy baseline
  using injury status, matchup, and recent news.
- **Waiver suggestions** — scores the free-agent pool with per-player metrics
  (mean / floor / ceiling / variance / risk / strategy fit / ROS value),
  pairs each pickup with a drop candidate, and asks Claude to re-rank.
- **Trade analyzer** — computes per-player and package metrics for the players
  you'd send and receive, gives a deterministic verdict
  (`ACCEPT` / `DECLINE` / `NEGOTIATE`) plus a Claude-written summary.
- **Draft manager** — polls draft state, identifies when you're on the clock,
  and asks Claude for the top 3 candidates with rationale.
- **Background scheduler** — refreshes roster, matchups, news, and draft
  state on a configurable interval (`settings.refresh_seconds`). Runs inside
  the GUI / TUI; can also be run standalone via `ff schedule` for cron.
- **Two front-ends:**
  - `ff-gui` — egui desktop GUI (Windows / macOS / Linux).
  - `ff` (or `ff ui`) — ratatui terminal UI (anywhere).
- **News ingest** — pulls public RSS feeds (ESPN, Sleeper) and filters to
  items mentioning rostered players before sending to Claude.

## Cross-platform install

Requires the **Rust 1.78+** toolchain.

### macOS / Linux

```sh
git clone https://github.com/epicsurf5200/groks_fantasy.git
cd groks_fantasy
./scripts/install.sh
```

> Linux desktop GUI needs `libxkbcommon`, `libwayland`, `libGL`, `libxcb`
> dev headers (e.g. `sudo apt install libxkbcommon-dev libwayland-dev
> libgl1-mesa-dev libxcb1-dev pkg-config`). The installer prints the
> right command for your distro. macOS needs only Xcode CLT.

### Windows

```bat
git clone https://github.com/epicsurf5200/groks_fantasy.git
cd groks_fantasy
scripts\install.bat
```

> Install [rustup](https://rustup.rs) first.

### Headless build (no GUI)

```sh
cargo build --release --bin ff --no-default-features
```

## Configure

```sh
./target/release/ff init sleeper      # or: espn, yahoo
$EDITOR config.yaml
export ANTHROPIC_API_KEY=sk-ant-...   # Windows: set ANTHROPIC_API_KEY=...
```

Sample configs live in [`examples/`](examples/).

### Provider notes

| Provider | Auth                                                                 |
| -------- | -------------------------------------------------------------------- |
| Sleeper  | None. Just `league_id` + (`username` or `user_id`).                  |
| ESPN     | `league_id`, `season`, `team_id`, plus `swid` + `espn_s2` cookies for private leagues. |
| Yahoo    | OAuth2 access token (issue out-of-band; refresh helper not bundled). |

## Run

### GUI (recommended)

```sh
# macOS / Linux
./scripts/run-gui.sh
# Windows
scripts\run-gui.bat
# or directly:
./target/release/ff-gui
```

The GUI has six tabs: **Roster · Lineup · Waiver · Trades · Draft · News**.
The strategy selector is in the top bar; "Refresh now" forces an immediate
data pull on top of the background schedule.

### Terminal UI

```sh
./target/release/ff               # default subcommand is `ui`
```

| Key       | Action                                                      |
| --------- | ----------------------------------------------------------- |
| `r`       | Force refresh (background scheduler runs continuously)      |
| `l`       | Generate AI lineup                                          |
| `w`       | Suggest waiver pickups                                      |
| `t`       | Switch to Trade tab; type partner / send / receive          |
| `d`       | Fetch draft state and AI pick suggestions                   |
| `s`       | Cycle strategy                                              |
| `Tab`/←→ | Switch tabs                                                  |
| `1`–`7`  | Jump to tab                                                  |
| `q`/Esc   | Quit                                                        |

### CLI subcommands

```sh
ff info                                # league summary
ff roster                              # text roster dump
ff lineup --week 7                     # AI lineup for any week
ff -s high_stakes lineup
ff waiver --pool 300                   # ranked waiver suggestions
ff trade --partner "Team B" \
         --send "Saquon Barkley, Brock Bowers" \
         --receive "Bijan Robinson, Sam LaPorta"
ff draft -i 5                          # poll draft every 5 s
ff draft-suggest                       # one-shot draft pick
ff schedule -i 600                     # standalone refresh loop (cron-friendly)
```

## How metrics are calculated

For every player we derive a `PlayerMetrics`:

| Field                | Definition                                                   |
| -------------------- | ------------------------------------------------------------ |
| `mean_projection`    | provider projection (or recent average if missing)           |
| `floor` / `ceiling`  | mean ± position-specific variance band                       |
| `variance`           | mean × position volatility (QB low, K/DST high)              |
| `risk_score`         | derived from injury status (Healthy 0.05 → IR 1.0)           |
| `injury_adj`         | strategy-aware multiplier (e.g. Conservative dings Q harder) |
| `adjusted_next_week` | `mean × injury_adj`                                          |
| `ros_value`          | `adjusted_next_week × 14` weeks                              |
| `trend`              | `(avg − projection) / mean` clamped to [-1, 1]               |
| `strategy_fit`       | 0..1 alignment with current strategy                          |

Trade verdicts compare summed `ros_value` of received vs sent and the
fairness ratio. Waiver candidates are scored by `(ros_upgrade) × (0.5 + 0.5 × fit)`
against the weakest roster player at the same position.

## Background scheduling

A single `Scheduler` shared between GUI and TUI refreshes:

- league settings, current week
- your roster + every other roster
- matchups for the current week
- RSS news (filtered to players on your roster)
- draft state

…on the interval set in `settings.refresh_seconds` (default 900 s = 15 min).
You can override per-run with `--config` or `ff schedule -i <seconds>`.

## Architecture

```
src/
├── main.rs              # clap CLI for the `ff` binary
├── lib.rs               # re-exports for the `ff-gui` binary
├── bin/gui_main.rs      # `ff-gui` entry point
├── ui.rs                # ratatui terminal UI (7 tabs)
├── gui.rs               # eframe/egui desktop GUI (6 tabs)
├── anthropic.rs         # Claude Messages API client
├── strategy.rs          # Conservative / Balanced / HighStakes guidance
├── lineup.rs            # greedy baseline + AI-refined optimizer
├── waiver.rs            # free-agent scoring + AI re-rank
├── trade.rs             # trade metrics + verdict + AI summary
├── draft.rs             # draft state polling + AI suggestion parser
├── metrics.rs           # PlayerMetrics + PackageMetrics
├── news.rs              # RSS fetch + roster filter
├── scheduler.rs         # background AppData refresh loop
├── types.rs             # Player / Roster / Matchup / DraftState
├── config.rs            # YAML config + env overrides
└── providers/
    ├── mod.rs           # async Provider trait
    ├── espn.rs          # ESPN lm-api (cookie auth)
    ├── sleeper.rs       # Sleeper public REST
    └── yahoo.rs         # Yahoo Fantasy v2 (OAuth bearer)
```

## Limitations

- ESPN/Yahoo write-back of lineups is not yet implemented; the app
  surfaces the recommendation and you apply it.
- ESPN free-agent listing requires the kona endpoint with
  `x-fantasy-filter` headers — currently a stub. The waiver analyzer
  falls back to AI-only suggestions when the pool is empty.
- Yahoo OAuth refresh is left to an external helper (token paste-in only).

## License

MIT
