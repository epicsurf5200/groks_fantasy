import Foundation

struct ServerConfig: Codable {
    var baseURL: String      // e.g. https://ff.example.com
    var bearerToken: String
}

enum APIError: LocalizedError {
    case http(Int, String)
    case decoding(String)
    case transport(String)

    var errorDescription: String? {
        switch self {
        case .http(let code, let body): return "HTTP \(code): \(body)"
        case .decoding(let m): return "decode: \(m)"
        case .transport(let m): return "network: \(m)"
        }
    }
}

actor APIClient {
    private let session: URLSession
    var config: ServerConfig

    init(config: ServerConfig) {
        self.config = config
        let cfg = URLSessionConfiguration.default
        cfg.timeoutIntervalForRequest = 60
        self.session = URLSession(configuration: cfg)
    }

    func update(config: ServerConfig) { self.config = config }

    func get<T: Decodable>(_ path: String) async throws -> T {
        try await request(path: path, method: "GET", body: Optional<EmptyBody>.none)
    }

    func post<Body: Encodable, T: Decodable>(_ path: String, body: Body) async throws -> T {
        try await request(path: path, method: "POST", body: body)
    }

    private func request<Body: Encodable, T: Decodable>(
        path: String, method: String, body: Body?
    ) async throws -> T {
        guard var components = URLComponents(string: config.baseURL) else {
            throw APIError.transport("bad base URL")
        }
        components.path = (components.path.isEmpty ? "" : components.path) + path
        guard let url = components.url else {
            throw APIError.transport("bad URL components")
        }
        var req = URLRequest(url: url)
        req.httpMethod = method
        req.setValue("Bearer \(config.bearerToken)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        if let body, !(body is EmptyBody) {
            req.httpBody = try JSONEncoder().encode(body)
        }
        let (data, response): (Data, URLResponse)
        do {
            (data, response) = try await session.data(for: req)
        } catch {
            throw APIError.transport(error.localizedDescription)
        }
        guard let http = response as? HTTPURLResponse else {
            throw APIError.transport("not HTTP")
        }
        guard (200..<300).contains(http.statusCode) else {
            let body = String(data: data, encoding: .utf8) ?? ""
            throw APIError.http(http.statusCode, body)
        }
        do {
            return try JSONDecoder().decode(T.self, from: data)
        } catch {
            throw APIError.decoding(error.localizedDescription)
        }
    }

    private struct EmptyBody: Encodable {}
}

// Endpoint convenience wrappers --------------------------------------------------

extension APIClient {
    func info() async throws -> InfoDTO { try await get("/api/info") }
    func roster() async throws -> RosterDTO? { try await get("/api/roster") }
    func news() async throws -> [NewsDTO] { try await get("/api/news") }
    func draft() async throws -> DraftStateDTO? { try await get("/api/draft") }

    struct LineupReq: Encodable { var week: Int? }
    func lineup(week: Int? = nil) async throws -> LineupDTO {
        try await post("/api/lineup", body: LineupReq(week: week))
    }

    struct WaiverReq: Encodable { var pool: Int? }
    func waivers(pool: Int = 200) async throws -> WaiverDTO {
        try await post("/api/waiver", body: WaiverReq(pool: pool))
    }

    struct TradeReq: Encodable {
        var partner: String; var send: [String]; var receive: [String]
    }
    func analyzeTrade(partner: String, send: [String], receive: [String]) async throws -> TradeDTO {
        try await post("/api/trade", body: TradeReq(partner: partner, send: send, receive: receive))
    }

    struct EmptyReq: Encodable {}
    func draftSuggest() async throws -> [DraftSuggestDTO] {
        try await post("/api/draft/suggest", body: EmptyReq())
    }

    struct StrategyReq: Encodable { var kind: String }
    func setStrategy(_ kind: String) async throws -> [String: String] {
        try await post("/api/strategy", body: StrategyReq(kind: kind))
    }
    func refreshNow() async throws -> [String: Bool] {
        try await post("/api/refresh", body: EmptyReq())
    }
}
