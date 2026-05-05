import SwiftUI

struct LineupView: View {
    @EnvironmentObject var store: AppStore
    @State private var working = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                List {
                    if let l = store.lineup {
                        Section(header: Text(String(format: "Week %d — projected %.1f", l.week, l.projectedTotal))) {
                            ForEach(l.starters, id: \.slot) { s in LineupSlotRow(slot: s) }
                        }
                        Section("Reasoning") {
                            Text(l.reasoning).font(.callout)
                        }
                    } else {
                        Section { Text("No lineup yet.") }
                    }
                }
                Divider()
                Button {
                    working = true
                    Task { await store.generateLineup(week: nil); working = false }
                } label: {
                    if working { ProgressView() } else {
                        Text("Generate AI lineup").bold().frame(maxWidth: .infinity)
                    }
                }
                .buttonStyle(.borderedProminent)
                .padding()
                .disabled(working)
            }
            .navigationTitle("Lineup")
            .padding(.top, 60)
        }
    }
}

struct LineupSlotRow: View {
    let slot: ULineupSlot
    var body: some View {
        HStack {
            Text(slot.slot).font(.caption.monospaced()).frame(width: 50, alignment: .leading)
            if let p = slot.player {
                VStack(alignment: .leading) {
                    Text(p.name)
                    Text("\(p.position) · \(p.team) · \(p.status)")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Text(String(format: "%.1f", p.projectedPoints))
                    .font(.body.monospaced())
            } else {
                Text("(empty)").italic().foregroundStyle(.secondary)
            }
        }
    }
}
