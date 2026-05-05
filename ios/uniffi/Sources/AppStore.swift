import Foundation
import SwiftUI

/// Bridge between SwiftUI and the Rust core (via UniFFI).
///
/// On first launch the user pastes their `config.yaml` (or imports it via
/// the file picker). The contents are stored in the app's keychain and
/// passed to `FfClient(configYaml:anthropicApiKeyOverride:)`. Periodic
/// refreshes are driven by the Rust scheduler running inside the FFI; the
/// UI just reads `client.roster()` / `client.news()` / etc.
@MainActor
final class AppStore: ObservableObject {
    @Published var hasClient: Bool = false
    @Published var statusMessage: String = "Paste your config.yaml to begin."
    @Published var strategyLabel: String = "Balanced"
    @Published var weekNumber: Int = 0
    @Published var lastRefresh: String = "—"

    @Published var roster: URoster?
    @Published var news: [UNewsItem] = []
    @Published var lineup: ULineup?
    @Published var waiver: UWaiverReport?
    @Published var trade: UTradeAnalysis?
    @Published var draftState: UDraftState?
    @Published var draftSuggestions: [UDraftSuggestion] = []

    private var client: FfClient?
    private var refreshTimer: Timer?

    func initialize(configYaml: String, apiKeyOverride: String?) {
        do {
            let client = try FfClient(
                configYaml: configYaml,
                anthropicApiKeyOverride: apiKeyOverride?.nilIfEmpty
            )
            self.client = client
            self.hasClient = true
            self.statusMessage = "Connected. Background refresh running."
            self.strategyLabel = client.currentStrategy()
            ConfigStorage.save(yaml: configYaml, apiKey: apiKeyOverride)
            self.startUiRefreshLoop()
        } catch {
            self.statusMessage = "init failed: \(error)"
        }
    }

    func bootstrapFromStorage() {
        if let saved = ConfigStorage.load() {
            initialize(configYaml: saved.yaml, apiKeyOverride: saved.apiKey)
        }
    }

    func setStrategy(_ kind: String) {
        guard let client else { return }
        do {
            try client.setStrategy(kind: kind)
            strategyLabel = client.currentStrategy()
        } catch {
            statusMessage = "strategy error: \(error)"
        }
    }

    func refreshNow() {
        client?.refreshNow()
        statusMessage = "Refresh requested."
    }

    private func startUiRefreshLoop() {
        refreshTimer?.invalidate()
        // Pull a snapshot from the Rust scheduler every 2 seconds.
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
        tick()
    }

    private func tick() {
        guard let client else { return }
        weekNumber = Int(client.currentWeek())
        if let secs = client.lastRefreshSecondsAgo() {
            lastRefresh = "\(secs)s ago"
        } else {
            lastRefresh = "never"
        }
        if let err = client.lastError() {
            statusMessage = "scheduler error: \(err)"
        }
        roster = client.roster()
        news = client.news(limit: 60)
        if let d = client.draftState() {
            draftState = d
        }
    }

    // -- AI calls (run off the main actor) -----------------------------------

    func generateLineup(week: UInt8?) async {
        guard let client else { return }
        statusMessage = "Asking Claude for lineup…"
        do {
            let result = try await Task.detached(priority: .userInitiated) { () throws -> ULineup in
                try client.lineup(weekOverride: week)
            }.value
            lineup = result
            statusMessage = String(format: "Lineup ready — projected %.1f", result.projectedTotal)
        } catch {
            statusMessage = "lineup error: \(error)"
        }
    }

    func suggestWaivers(pool: UInt32 = 200) async {
        guard let client else { return }
        statusMessage = "Scoring free agents…"
        do {
            let result = try await Task.detached(priority: .userInitiated) { () throws -> UWaiverReport in
                try client.waivers(pool: pool)
            }.value
            waiver = result
            statusMessage = "\(result.candidates.count) waiver candidates."
        } catch {
            statusMessage = "waiver error: \(error)"
        }
    }

    func analyzeTrade(partner: String, send: String, receive: String) async {
        guard let client else { return }
        statusMessage = "Computing trade metrics…"
        do {
            let result = try await Task.detached(priority: .userInitiated) { () throws -> UTradeAnalysis in
                try client.analyzeTrade(partner: partner, sendCsv: send, receiveCsv: receive)
            }.value
            trade = result
            statusMessage = String(format: "%@  net ROS %+.1f", result.verdict, result.netRosDelta)
        } catch {
            statusMessage = "trade error: \(error)"
        }
    }

    func draftSuggest() async {
        guard let client else { return }
        statusMessage = "Asking Claude for next picks…"
        do {
            let result = try await Task.detached(priority: .userInitiated) { () throws -> [UDraftSuggestion] in
                try client.draftSuggestions()
            }.value
            draftSuggestions = result
            statusMessage = "\(result.count) draft candidates."
        } catch {
            statusMessage = "draft error: \(error)"
        }
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
