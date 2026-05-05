import SwiftUI

@main
struct GroksFantasyRemoteApp: App {
    @StateObject private var store = RemoteStore()
    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(store)
                .onAppear { store.bootstrap() }
        }
    }
}

struct RootView: View {
    @EnvironmentObject var store: RemoteStore
    var body: some View {
        if store.connected {
            MainTabs()
        } else {
            ConnectView()
        }
    }
}

struct ConnectView: View {
    @EnvironmentObject var store: RemoteStore
    @State private var baseURL = "https://"
    @State private var token = ""
    @State private var working = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Server") {
                    TextField("https://your-server.example.com", text: $baseURL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                    SecureField("Bearer token", text: $token)
                }
                Section {
                    Button {
                        working = true
                        Task {
                            await store.connect(baseURL: baseURL, token: token)
                            working = false
                        }
                    } label: {
                        if working { ProgressView() } else {
                            Text("Connect").frame(maxWidth: .infinity)
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(baseURL.isEmpty || token.isEmpty || working)
                }
                Section {
                    Text(store.status).font(.footnote).foregroundStyle(.secondary)
                }
            }
            .navigationTitle("groks_fantasy")
        }
    }
}

struct MainTabs: View {
    var body: some View {
        TabView {
            RemoteRosterView().tabItem { Label("Roster", systemImage: "person.3") }
            RemoteLineupView().tabItem { Label("Lineup", systemImage: "list.number") }
            RemoteWaiverView().tabItem { Label("Waiver", systemImage: "arrow.up.arrow.down.circle") }
            RemoteTradeView().tabItem { Label("Trade", systemImage: "arrow.triangle.swap") }
            RemoteDraftView().tabItem { Label("Draft", systemImage: "calendar") }
            RemoteNewsView().tabItem { Label("News", systemImage: "newspaper") }
            RemoteSettingsView().tabItem { Label("Settings", systemImage: "gearshape") }
        }
        .overlay(alignment: .top) { TopBar() }
    }
}

struct TopBar: View {
    @EnvironmentObject var store: RemoteStore
    var body: some View {
        VStack(spacing: 4) {
            HStack {
                Text("wk \(store.info?.week ?? 0)").font(.caption.monospaced())
                Spacer()
                Text(store.info?.strategy ?? "—").font(.caption.bold())
                Spacer()
                Text("\(store.info?.last_refresh_seconds_ago.map { "\($0)s" } ?? "—")")
                    .font(.caption.monospaced())
                Button { Task { await store.refreshNow() } } label: {
                    Image(systemName: "arrow.clockwise")
                }.buttonStyle(.borderless)
            }
            .padding(.horizontal).padding(.vertical, 4).background(.ultraThinMaterial)
            Text(store.status).font(.caption2).foregroundStyle(.secondary).lineLimit(2).padding(.horizontal)
        }
    }
}

struct RemoteRosterView: View {
    @EnvironmentObject var store: RemoteStore
    var body: some View {
        NavigationStack {
            if let r = store.roster {
                List {
                    Section {
                        Text("\(r.team_name)  \(r.wins)-\(r.losses)-\(r.ties)").font(.headline)
                        Text(String(format: "PF %.1f · PA %.1f", r.points_for, r.points_against))
                            .font(.subheadline).foregroundStyle(.secondary)
                    }
                    Section("Players") {
                        ForEach(r.players) { p in
                            HStack {
                                Text(p.roster_slot).font(.caption.monospaced()).frame(width: 38, alignment: .leading)
                                VStack(alignment: .leading) {
                                    Text(p.name)
                                    Text("\(p.position) · \(p.team)").font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Text(p.status).font(.caption.bold())
                                Text(String(format: "%.1f", p.projected_points)).font(.body.monospaced())
                            }
                        }
                    }
                }
                .navigationTitle("Roster")
                .padding(.top, 60)
            } else {
                ProgressView().padding(.top, 60)
            }
        }
    }
}

struct RemoteLineupView: View {
    @EnvironmentObject var store: RemoteStore
    @State private var working = false
    var body: some View {
        NavigationStack {
            VStack {
                List {
                    if let l = store.lineup {
                        Section(header: Text(String(format: "Week %d — projected %.1f", l.week, l.projected_total))) {
                            ForEach(l.starters) { s in
                                HStack {
                                    Text(s.slot).font(.caption.monospaced()).frame(width: 50, alignment: .leading)
                                    if let p = s.player {
                                        VStack(alignment: .leading) {
                                            Text(p.name)
                                            Text("\(p.position) · \(p.team) · \(p.status)").font(.caption).foregroundStyle(.secondary)
                                        }
                                        Spacer()
                                        Text(String(format: "%.1f", p.projected_points)).font(.body.monospaced())
                                    } else {
                                        Text("(empty)").italic().foregroundStyle(.secondary)
                                    }
                                }
                            }
                        }
                        Section("Reasoning") { Text(l.reasoning).font(.callout) }
                    } else {
                        Text("No lineup yet.")
                    }
                }
                Button {
                    working = true
                    Task { await store.generateLineup(); working = false }
                } label: {
                    if working { ProgressView() } else {
                        Text("Generate AI lineup").bold().frame(maxWidth: .infinity)
                    }
                }
                .buttonStyle(.borderedProminent).padding().disabled(working)
            }
            .navigationTitle("Lineup")
            .padding(.top, 60)
        }
    }
}

struct RemoteWaiverView: View {
    @EnvironmentObject var store: RemoteStore
    @State private var working = false
    var body: some View {
        NavigationStack {
            VStack {
                List {
                    if let r = store.waiver {
                        Section("Candidates") {
                            ForEach(r.candidates) { c in
                                VStack(alignment: .leading) {
                                    HStack {
                                        Text("#\(c.priority)").font(.caption.monospaced())
                                        Text(c.player.name).font(.body.bold())
                                        Spacer()
                                        Text(String(format: "ROS %.0f", c.metrics.ros_value)).font(.callout.monospaced())
                                    }
                                    Text(String(format: "%@ %@ · floor %.1f · ceil %.1f · risk %.2f · fit %.2f",
                                                c.player.position, c.player.team,
                                                c.metrics.floor, c.metrics.ceiling,
                                                c.metrics.risk_score, c.metrics.strategy_fit))
                                        .font(.caption.monospaced()).foregroundStyle(.secondary)
                                    if let drop = c.drop_player_name {
                                        Text("drop \(drop) (Δ \(String(format: "%+.0f", c.drop_ros_delta)))")
                                            .font(.caption).foregroundStyle(.orange)
                                    }
                                }
                            }
                        }
                        if !r.summary.isEmpty {
                            Section("Claude analysis") { Text(r.summary).font(.callout) }
                        }
                    } else { Text("No suggestions yet.") }
                }
                Button {
                    working = true
                    Task { await store.suggestWaivers(); working = false }
                } label: {
                    if working { ProgressView() } else {
                        Text("Suggest waiver pickups").bold().frame(maxWidth: .infinity)
                    }
                }
                .buttonStyle(.borderedProminent).padding().disabled(working)
            }
            .navigationTitle("Waiver")
            .padding(.top, 60)
        }
    }
}

struct RemoteTradeView: View {
    @EnvironmentObject var store: RemoteStore
    @State private var partner = ""
    @State private var send = ""
    @State private var receive = ""
    @State private var working = false
    var body: some View {
        NavigationStack {
            Form {
                Section("Proposal") {
                    TextField("Partner team", text: $partner).autocorrectionDisabled()
                    TextField("Send (comma)", text: $send).autocorrectionDisabled()
                    TextField("Receive (comma)", text: $receive).autocorrectionDisabled()
                    Button {
                        working = true
                        Task {
                            let s = send.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
                            let r = receive.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
                            await store.analyzeTrade(partner: partner, send: s, receive: r)
                            working = false
                        }
                    } label: {
                        if working { ProgressView() } else {
                            Text("Analyze").frame(maxWidth: .infinity)
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(working)
                }
                if let t = store.trade {
                    Section("Verdict") {
                        Text(t.verdict).font(.title3.bold())
                            .foregroundStyle(t.verdict == "ACCEPT" ? .green : t.verdict == "DECLINE" ? .red : .yellow)
                        Text(String(format: "net ROS %+.1f · fairness %.0f%%",
                                    t.net_ros_delta, t.fairness * 100))
                            .font(.caption.monospaced())
                    }
                    Section("You send") {
                        ForEach(Array(t.send_names.enumerated()), id: \.offset) { i, n in
                            VStack(alignment: .leading) {
                                Text(n)
                                Text(String(format: "mean %.1f · floor %.1f · ceil %.1f · ROS %.0f · risk %.2f",
                                            t.send[i].mean_projection, t.send[i].floor,
                                            t.send[i].ceiling, t.send[i].ros_value, t.send[i].risk_score))
                                    .font(.caption.monospaced()).foregroundStyle(.secondary)
                            }
                        }
                    }
                    Section("You receive") {
                        ForEach(Array(t.receive_names.enumerated()), id: \.offset) { i, n in
                            VStack(alignment: .leading) {
                                Text(n)
                                Text(String(format: "mean %.1f · floor %.1f · ceil %.1f · ROS %.0f · risk %.2f",
                                            t.receive[i].mean_projection, t.receive[i].floor,
                                            t.receive[i].ceiling, t.receive[i].ros_value, t.receive[i].risk_score))
                                    .font(.caption.monospaced()).foregroundStyle(.secondary)
                            }
                        }
                    }
                    if !t.summary.isEmpty {
                        Section("Claude analysis") { Text(t.summary).font(.callout) }
                    }
                }
            }
            .navigationTitle("Trade")
            .padding(.top, 60)
        }
    }
}

struct RemoteDraftView: View {
    @EnvironmentObject var store: RemoteStore
    @State private var working = false
    var body: some View {
        NavigationStack {
            VStack {
                List {
                    if let d = store.draftState {
                        Text("Pick #\(d.current_pick)  R\((d.current_pick - 1) / max(d.team_count, 1) + 1)/\(d.total_rounds)  \(d.team_count) teams")
                            .font(.subheadline.monospaced())
                        Section("Recent picks") {
                            ForEach(Array(d.picks.suffix(20).reversed())) { p in
                                HStack {
                                    Text("R\(p.round).\(p.pick_number)").font(.caption.monospaced()).frame(width: 60, alignment: .leading)
                                    Text(p.team_name).font(.caption)
                                    Spacer()
                                    Text(p.player_name ?? "—").font(.caption.bold())
                                }
                            }
                        }
                    } else { Text("No draft state.") }
                    if !store.draftSuggestions.isEmpty {
                        Section("Top 3 candidates") {
                            ForEach(store.draftSuggestions) { s in
                                VStack(alignment: .leading) {
                                    Text("\(s.rank). \(s.name) (\(s.position ?? "?"))").font(.body.bold())
                                    Text(s.rationale).font(.caption)
                                }
                            }
                        }
                    }
                }
                Button {
                    working = true
                    Task { await store.draftSuggest(); working = false }
                } label: {
                    if working { ProgressView() } else {
                        Text("Suggest next pick").bold().frame(maxWidth: .infinity)
                    }
                }
                .buttonStyle(.borderedProminent).padding().disabled(working)
            }
            .navigationTitle("Draft")
            .padding(.top, 60)
        }
    }
}

struct RemoteNewsView: View {
    @EnvironmentObject var store: RemoteStore
    var body: some View {
        NavigationStack {
            List(store.news) { n in
                VStack(alignment: .leading, spacing: 4) {
                    Text("[\(n.source)]").font(.caption.bold()).foregroundStyle(.cyan)
                    if let url = URL(string: n.url), !n.url.isEmpty {
                        Link(n.title, destination: url)
                    } else {
                        Text(n.title)
                    }
                }
            }
            .navigationTitle("News")
            .padding(.top, 60)
        }
    }
}

struct RemoteSettingsView: View {
    @EnvironmentObject var store: RemoteStore
    var body: some View {
        NavigationStack {
            Form {
                Section("Strategy") {
                    Picker("Mode", selection: Binding(
                        get: { store.info?.strategy ?? "Balanced" },
                        set: { newLabel in Task { await store.setStrategy(strategyKey(for: newLabel)) } }
                    )) {
                        Text("Conservative").tag("Conservative")
                        Text("Balanced").tag("Balanced")
                        Text("High Stakes / High Rewards").tag("High Stakes / High Rewards")
                    }
                }
                Section("Connection") {
                    Button("Refresh now") { Task { await store.refreshNow() } }
                    Button("Disconnect", role: .destructive) { store.disconnect() }
                }
            }
            .navigationTitle("Settings")
            .padding(.top, 60)
        }
    }
}

private func strategyKey(for label: String) -> String {
    switch label {
    case "Conservative": return "conservative"
    case "Balanced": return "balanced"
    default: return "high_stakes"
    }
}
