import SwiftUI

struct DraftView: View {
    @EnvironmentObject var store: AppStore
    @State private var working = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                List {
                    if let d = store.draftState {
                        Section {
                            Text(String(
                                format: "Pick #%d  R%d/%d  %d teams  %@",
                                d.currentPick,
                                ((d.currentPick - 1) / max(d.teamCount, 1) + 1),
                                d.totalRounds,
                                d.teamCount,
                                d.completed ? "complete" : "in progress"
                            )).font(.subheadline.monospaced())
                        }
                        Section("Recent picks") {
                            ForEach(Array(d.picks.suffix(20).reversed().enumerated()), id: \.offset) { _, p in
                                HStack {
                                    Text("R\(p.round).\(p.pickNumber)")
                                        .font(.caption.monospaced())
                                        .frame(width: 60, alignment: .leading)
                                    Text(p.teamName).font(.caption)
                                    Spacer()
                                    Text(p.playerName ?? "—").font(.caption.bold())
                                }
                            }
                        }
                    } else {
                        Section { Text("No draft state.") }
                    }
                    if !store.draftSuggestions.isEmpty {
                        Section("Top 3 candidates") {
                            ForEach(store.draftSuggestions, id: \.rank) { s in
                                VStack(alignment: .leading, spacing: 2) {
                                    HStack {
                                        Text("#\(s.rank)").font(.caption.monospaced())
                                        Text(s.name).font(.body.bold())
                                        Text(s.position ?? "?").font(.caption).foregroundStyle(.secondary)
                                    }
                                    Text(s.rationale).font(.caption)
                                }
                            }
                        }
                    }
                }
                Divider()
                Button {
                    working = true
                    Task { await store.draftSuggest(); working = false }
                } label: {
                    if working { ProgressView() } else {
                        Text("Suggest next pick").bold().frame(maxWidth: .infinity)
                    }
                }
                .buttonStyle(.borderedProminent)
                .padding()
                .disabled(working)
            }
            .navigationTitle("Draft")
            .padding(.top, 60)
        }
    }
}
