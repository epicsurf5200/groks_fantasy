import SwiftUI

struct RosterView: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        NavigationStack {
            if let r = store.roster {
                List {
                    Section {
                        Text("\(r.teamName)  \(r.wins)-\(r.losses)-\(r.ties)")
                            .font(.headline)
                        Text(String(format: "PF %.1f · PA %.1f", r.pointsFor, r.pointsAgainst))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                    Section("Players") {
                        ForEach(r.players, id: \.id) { p in PlayerRow(player: p) }
                    }
                }
                .navigationTitle("Roster")
                .padding(.top, 60)
            } else {
                ProgressView("Waiting for refresh…")
                    .padding(.top, 60)
            }
        }
    }
}

struct PlayerRow: View {
    let player: UPlayer
    var body: some View {
        HStack {
            Text(player.rosterSlot).font(.caption.monospaced()).frame(width: 38, alignment: .leading)
            VStack(alignment: .leading) {
                Text(player.name).font(.body)
                Text("\(player.position) · \(player.team)").font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Text(statusLabel).font(.caption.bold()).foregroundStyle(statusColor)
            Text(String(format: "%.1f", player.projectedPoints))
                .font(.body.monospaced())
                .frame(width: 56, alignment: .trailing)
        }
    }
    private var statusLabel: String { player.status }
    private var statusColor: Color {
        switch player.status {
        case "OUT", "IR", "SUSP": return .red
        case "D": return .orange
        case "Q": return .yellow
        default: return .secondary
        }
    }
}
