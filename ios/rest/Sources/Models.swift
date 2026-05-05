import Foundation

// Codable mirrors of the JSON shapes returned by ff-server.

struct InfoDTO: Codable {
    let provider: String
    let week: Int
    let strategy: String
    let last_refresh_seconds_ago: Int?
    let last_error: String?
}

struct PlayerDTO: Codable, Identifiable, Hashable {
    let id: String
    let name: String
    let position: String
    let roster_slot: String
    let team: String
    let status: String
    let projected_points: Double
    let avg_points: Double
}

struct RosterDTO: Codable {
    let team_id: String
    let team_name: String
    let wins: Int
    let losses: Int
    let ties: Int
    let points_for: Double
    let points_against: Double
    let players: [PlayerDTO]
}

struct LineupSlotDTO: Codable, Identifiable {
    let slot: String
    let player: PlayerDTO?
    var id: String { slot + (player?.id ?? "") }
}

struct LineupDTO: Codable {
    let week: Int
    let starters: [LineupSlotDTO]
    let bench: [PlayerDTO]
    let projected_total: Double
    let reasoning: String
}

struct MetricsDTO: Codable {
    let mean_projection: Double
    let floor: Double
    let ceiling: Double
    let risk_score: Double
    let ros_value: Double
    let strategy_fit: Double
}

struct WaiverCandDTO: Codable, Identifiable {
    let priority: Int
    let player: PlayerDTO
    let metrics: MetricsDTO
    let drop_player_name: String?
    let drop_ros_delta: Double
    let reasoning: String
    var id: String { "\(priority)-\(player.id)" }
}

struct WaiverDTO: Codable {
    let candidates: [WaiverCandDTO]
    let summary: String
}

struct TradeDTO: Codable {
    let verdict: String
    let net_ros_delta: Double
    let fairness: Double
    let send: [MetricsDTO]
    let send_names: [String]
    let receive: [MetricsDTO]
    let receive_names: [String]
    let send_total_ros: Double
    let receive_total_ros: Double
    let summary: String
}

struct DraftPickDTO: Codable, Identifiable {
    let pick_number: Int
    let round: Int
    let team_name: String
    let player_name: String?
    let position: String?
    var id: Int { pick_number }
}

struct DraftStateDTO: Codable {
    let picks: [DraftPickDTO]
    let current_pick: Int
    let total_rounds: Int
    let team_count: Int
    let completed: Bool
}

struct DraftSuggestDTO: Codable, Identifiable {
    let rank: Int
    let name: String
    let position: String?
    let rationale: String
    var id: Int { rank }
}

struct NewsDTO: Codable, Identifiable {
    let title: String
    let summary: String
    let source: String
    let url: String
    var id: String { url.isEmpty ? title : url }
}
