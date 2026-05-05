import SwiftUI

struct RootView: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        Group {
            if store.hasClient {
                MainTabs()
            } else {
                OnboardingView()
            }
        }
        .onAppear { store.bootstrapFromStorage() }
    }
}

struct MainTabs: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        TabView {
            RosterView().tabItem { Label("Roster", systemImage: "person.3") }
            LineupView().tabItem { Label("Lineup", systemImage: "list.number") }
            WaiverView().tabItem { Label("Waiver", systemImage: "arrow.up.arrow.down.circle") }
            TradeView().tabItem { Label("Trade", systemImage: "arrow.triangle.swap") }
            DraftView().tabItem { Label("Draft", systemImage: "calendar") }
            NewsView().tabItem { Label("News", systemImage: "newspaper") }
            SettingsView().tabItem { Label("Settings", systemImage: "gearshape") }
        }
        .overlay(alignment: .top) { TopBar() }
    }
}

struct TopBar: View {
    @EnvironmentObject var store: AppStore
    var body: some View {
        VStack(spacing: 4) {
            HStack {
                Text("wk \(store.weekNumber)")
                    .font(.caption.monospaced())
                Spacer()
                Text(store.strategyLabel).font(.caption.bold())
                Spacer()
                Text("refresh \(store.lastRefresh)").font(.caption.monospaced())
                Button(action: { store.refreshNow() }) {
                    Image(systemName: "arrow.clockwise")
                }.buttonStyle(.borderless)
            }
            .padding(.horizontal)
            .padding(.vertical, 4)
            .background(.ultraThinMaterial)
            Text(store.statusMessage)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(2)
                .padding(.horizontal)
        }
    }
}
