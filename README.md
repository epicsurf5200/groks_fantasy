# groks_fantasy

An autonomous, **Claude-powered** fantasy-football manager. Pulls your roster,
matchups, projections, and live news; recommends or sets a starting lineup
according to a user-chosen strategy; and runs an interactive draft assistant.

> Despite the name, the LLM provider is **Anthropic Claude** — not Grok.

## Features

- **Multi-provider:** ESPN, Sleeper (no-auth public API), and Yahoo (OAuth2 token).
- **Strategy modes:**
  - `conservative` — floor over ceiling, healthy starters, avoid Q tags.
  - `balanced` — risk-adjusted projections (default).
  - `high_stakes` — maximize ceiling, lean into shootouts and breakouts.
- **AI lineup optimizer** — Claude refines a deterministic greedy baseline
  using injury status, matchup, and recent news.
- **Draft manager** — polls draft state, identifies when you're on the clock,
  and asks Claude for the top 3 candidates with rationale.
- **Terminal UI** — five-tab TUI built on `ratatui` (Roster / Lineup /
  Draft / News / Help).
- **News ingest** — pulls public RSS feeds (ESPN, Sleeper) and filters to
  items mentioning rostered players before sending to Claude.

## Install

```sh
# Requires Rust 1.78+
git clone https://github.com/epicsurf5200/groks_fantasy.git
cd groks_fantasy
cargo build --release
# Binary at ./target/release/ff
```

## Configure

```sh
./target/release/ff init sleeper      # or: espn, yahoo
$EDITOR config.yaml
export ANTHROPIC_API_KEY=sk-ant-...
```

Sample configs live in [`examples/`](examples/).

### Provider notes

| Provider | Auth                                                             |
| -------- | ---------------------------------------------------------------- |
| Sleeper  | None. Just `league_id` + (`username` or `user_id`).              |
| ESPN     | `league_id`, `season`, `team_id`, plus `swid` + `espn_s2` cookies for private leagues. |
| Yahoo    | OAuth2 access token (issue out-of-band; refresh helper not bundled). |

## Run

```sh
ff                            # launch TUI (default)
ff info                       # league summary
ff roster                     # text roster dump
ff lineup                     # AI lineup for current week
ff lineup --week 7
ff -s high_stakes lineup
ff draft-suggest              # one-shot draft picks
ff draft -i 5                 # poll draft every 5s
```

### TUI keys

| Key      | Action                                                    |
| -------- | --------------------------------------------------------- |
| `r`      | Refresh roster, matchups, news                            |
| `l`      | Generate AI lineup for current week                       |
| `d`      | Fetch draft state and AI pick suggestions                 |
| `s`      | Cycle strategy (Conservative → Balanced → High Stakes)    |
| `Tab`/←→ | Switch tabs                                               |
| `1`–`5`  | Jump to tab                                               |
| `q`/Esc  | Quit                                                      |

## Architecture

```
src/
├── main.rs            # clap CLI + subcommand dispatch
├── ui.rs              # ratatui TUI (5 tabs)
├── anthropic.rs       # Claude Messages API client
├── strategy.rs        # Conservative / Balanced / HighStakes guidance
├── lineup.rs          # greedy baseline + AI-refined optimizer
├── draft.rs           # draft state polling + AI suggestion parser
├── news.rs            # RSS fetch + roster filter
├── types.rs           # Player / Roster / Matchup / DraftState
├── config.rs          # YAML config + env overrides
└── providers/
    ├── mod.rs         # async Provider trait
    ├── espn.rs        # ESPN lm-api (cookie auth)
    ├── sleeper.rs     # Sleeper public REST
    └── yahoo.rs       # Yahoo Fantasy v2 (OAuth bearer)
```

## Limitations

- ESPN and Yahoo write-back of lineups is not yet implemented; the app
  prints / displays the recommendation and you apply it.
- ESPN free-agent listing requires the kona endpoint with `x-fantasy-filter`
  headers and is currently a stub.
- Yahoo OAuth refresh is left to an external helper (token paste-in only).

## License

MIT
