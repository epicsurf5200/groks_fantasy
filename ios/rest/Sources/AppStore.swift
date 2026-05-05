import Foundation
import Security
import SwiftUI

@MainActor
final class RemoteStore: ObservableObject {
    @Published var connected: Bool = false
    @Published var status: String = "Configure your server URL and bearer token."
    @Published var info: InfoDTO?
    @Published var roster: RosterDTO?
    @Published var news: [NewsDTO] = []
    @Published var lineup: LineupDTO?
    @Published var waiver: WaiverDTO?
    @Published var trade: TradeDTO?
    @Published var draftState: DraftStateDTO?
    @Published var draftSuggestions: [DraftSuggestDTO] = []

    private var client: APIClient?
    private var pollTimer: Timer?

    func connect(baseURL: String, token: String) async {
        let cfg = ServerConfig(baseURL: baseURL, bearerToken: token)
        let client = APIClient(config: cfg)
        do {
            self.client = client
            let _: InfoDTO = try await client.info()
            self.connected = true
            self.status = "Connected to \(baseURL)."
            ServerStorage.save(cfg)
            await refreshAll()
            startPolling()
        } catch {
            self.connected = false
            self.client = nil
            self.status = "connect failed: \(error.localizedDescription)"
        }
    }

    func bootstrap() {
        if let cfg = ServerStorage.load() {
            Task { await connect(baseURL: cfg.baseURL, token: cfg.bearerToken) }
        }
    }

    func disconnect() {
        ServerStorage.clear()
        connected = false
        client = nil
        pollTimer?.invalidate()
    }

    private func startPolling() {
        pollTimer?.invalidate()
        pollTimer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
            Task { @MainActor in await self?.refreshAll() }
        }
    }

    func refreshAll() async {
        guard let client else { return }
        async let info: InfoDTO? = try? await client.info()
        async let roster: RosterDTO? = try? await client.roster()
        async let news: [NewsDTO]? = try? await client.news()
        async let draft: DraftStateDTO? = try? await client.draft()
        let i = await info; let r = await roster
        let n = await news; let d = await draft
        if let i { self.info = i }
        self.roster = r ?? self.roster
        self.news = n ?? self.news
        self.draftState = d ?? self.draftState
    }

    func refreshNow() async {
        guard let client else { return }
        do {
            _ = try await client.refreshNow()
            status = "Server refresh requested."
            await refreshAll()
        } catch {
            status = "refresh error: \(error.localizedDescription)"
        }
    }

    func setStrategy(_ kind: String) async {
        guard let client else { return }
        do {
            let resp = try await client.setStrategy(kind)
            status = "Strategy → \(resp["strategy"] ?? kind)"
            await refreshAll()
        } catch {
            status = "strategy error: \(error.localizedDescription)"
        }
    }

    func generateLineup(week: Int? = nil) async {
        guard let client else { return }
        status = "Server generating lineup…"
        do {
            lineup = try await client.lineup(week: week)
            status = String(format: "Lineup ready — projected %.1f", lineup?.projected_total ?? 0)
        } catch {
            status = "lineup error: \(error.localizedDescription)"
        }
    }

    func suggestWaivers() async {
        guard let client else { return }
        status = "Server scoring free agents…"
        do {
            waiver = try await client.waivers()
            status = "\(waiver?.candidates.count ?? 0) waiver candidates."
        } catch {
            status = "waiver error: \(error.localizedDescription)"
        }
    }

    func analyzeTrade(partner: String, send: [String], receive: [String]) async {
        guard let client else { return }
        status = "Server computing trade metrics…"
        do {
            trade = try await client.analyzeTrade(partner: partner, send: send, receive: receive)
            if let t = trade {
                status = String(format: "%@  net ROS %+.1f", t.verdict, t.net_ros_delta)
            }
        } catch {
            status = "trade error: \(error.localizedDescription)"
        }
    }

    func draftSuggest() async {
        guard let client else { return }
        status = "Server asking Claude for picks…"
        do {
            draftSuggestions = try await client.draftSuggest()
            status = "\(draftSuggestions.count) draft candidates."
        } catch {
            status = "draft error: \(error.localizedDescription)"
        }
    }
}

// Persisted server connection in the keychain.
enum ServerStorage {
    private static let service = "com.groksfantasy.remote"
    private static let key = "server.config"
    static func save(_ cfg: ServerConfig) {
        let data = (try? JSONEncoder().encode(cfg)) ?? Data()
        let q: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                kSecAttrService as String: service,
                                kSecAttrAccount as String: key]
        SecItemDelete(q as CFDictionary)
        var attrs = q
        attrs[kSecValueData as String] = data
        attrs[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
        SecItemAdd(attrs as CFDictionary, nil)
    }
    static func load() -> ServerConfig? {
        let q: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                kSecAttrService as String: service,
                                kSecAttrAccount as String: key,
                                kSecReturnData as String: true,
                                kSecMatchLimit as String: kSecMatchLimitOne]
        var item: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data,
              let cfg = try? JSONDecoder().decode(ServerConfig.self, from: data) else {
            return nil
        }
        return cfg
    }
    static func clear() {
        let q: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                kSecAttrService as String: service,
                                kSecAttrAccount as String: key]
        SecItemDelete(q as CFDictionary)
    }
}
