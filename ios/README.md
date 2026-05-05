# iOS apps

Two flavors live here. Both expect macOS + Xcode 15+ to build.

| Flavor                | Talks to                                          | When to use                                                   |
| --------------------- | ------------------------------------------------- | ------------------------------------------------------------- |
| `ios/uniffi/`         | Rust core compiled into the app via UniFFI       | You want a single self-contained iOS app — no server.         |
| `ios/rest/`           | A running `ff-server` over HTTPS + bearer token  | You already have a server (or want push, multi-device sync). |

Both apps are described in XcodeGen's `project.yml` format. Generate the
`.xcodeproj` with:

```sh
brew install xcodegen
cd ios/uniffi   # or ios/rest
xcodegen generate
open *.xcodeproj
```

## Option 1 — UniFFI (Rust core in-app)

### Build the XCFramework + Swift bindings (macOS)

```sh
# from repo root, on macOS:
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
./scripts/build-ios.sh
```

This produces:

- `ios/uniffi/Frameworks/FFCore.xcframework`
- `ios/uniffi/Generated/*.swift` (UniFFI-generated bindings)

XcodeGen wires the framework + sources into the app target.

### Run

```sh
cd ios/uniffi
xcodegen generate
open GroksFantasyCore.xcodeproj
# In Xcode: select an iPhone simulator, Cmd-R.
```

### How it works

- App bundles `FFCore.xcframework` (a `staticlib` Rust artifact wrapped per
  iOS slice) plus the UniFFI-generated `.swift` shim.
- On first launch the user pastes their `config.yaml`. Contents are stored
  in the iOS keychain.
- `FfClient` constructor brings up a Tokio runtime, builds the provider /
  Anthropic client / news fetcher, and starts the background `Scheduler`.
- All FFI methods are synchronous to Swift (they `block_on` the Tokio
  runtime internally). Swift wraps every call in `Task.detached { ... }`
  so the main thread never blocks.

### Files

```
ios/uniffi/
├── project.yml             # XcodeGen project description
├── Frameworks/FFCore.xcframework   (generated)
├── Generated/              (generated)
└── Sources/
    ├── Info.plist
    ├── App.swift           # @main
    ├── AppStore.swift      # ObservableObject bridging FfClient → SwiftUI
    ├── ConfigStorage.swift # Keychain helpers
    ├── RootView.swift
    ├── OnboardingView.swift
    ├── RosterView.swift
    ├── LineupView.swift
    ├── WaiverView.swift
    ├── TradeView.swift
    ├── DraftView.swift
    ├── NewsView.swift
    └── SettingsView.swift
```

## Option 2 — REST client (talks to ff-server)

### Run the server

```sh
# from repo root:
cargo run --release -p ff-server -- \
    --config config.yaml \
    --token "$(openssl rand -hex 32)" \
    --bind 0.0.0.0:8088
```

A starter `systemd` unit lives at [`scripts/ff-server.service`](../scripts/ff-server.service).
Front it with Caddy / nginx for TLS; iOS App Transport Security requires
HTTPS for any non-loopback host.

### Endpoints

| Method | Path                  | Notes                                      |
| ------ | --------------------- | ------------------------------------------ |
| `GET`  | `/health`             | Anonymous; returns `{ok:true}`             |
| `GET`  | `/api/info`           | Provider, week, strategy, scheduler state  |
| `GET`  | `/api/roster`         | Your roster                                |
| `GET`  | `/api/all-rosters`    | Whole league                               |
| `GET`  | `/api/news`           | Roster-filtered news                       |
| `POST` | `/api/lineup`         | `{week?: int}` → AI lineup                 |
| `POST` | `/api/waiver`         | `{pool?: int}` → ranked candidates         |
| `POST` | `/api/trade`          | `{partner, send: [], receive: []}`         |
| `GET`  | `/api/draft`          | Draft snapshot                             |
| `POST` | `/api/draft/suggest`  | AI top-3 candidates                        |
| `POST` | `/api/strategy`       | `{kind: "conservative|balanced|high_stakes"}` |
| `POST` | `/api/refresh`        | Force scheduler poke                       |

All authenticated routes require `Authorization: Bearer <token>`.

### Run the iOS app

```sh
cd ios/rest
xcodegen generate
open GroksFantasyRemote.xcodeproj
```

On first launch the app prompts for your server URL and bearer token; both
are stored in the keychain.

### Files

```
ios/rest/
├── project.yml
└── Sources/
    ├── Info.plist
    ├── Models.swift        # Codable mirrors of the JSON schema
    ├── APIClient.swift     # actor-isolated URLSession client
    ├── AppStore.swift      # ObservableObject + keychain
    └── Views.swift         # @main + every SwiftUI screen
```

## Troubleshooting

- **App Transport Security blocks HTTP localhost** — `Info.plist` already
  sets `NSAllowsLocalNetworking=true`. Use `https://` for production.
- **UniFFI bindings out of date** — re-run `./scripts/build-ios.sh`.
- **Linker errors about `_OBJC_CLASS_$_…`** — confirm that
  `FRAMEWORK_SEARCH_PATHS` in `project.yml` points at `Frameworks/` and
  that the target embeds `FFCore.xcframework` (XcodeGen does this for you).
