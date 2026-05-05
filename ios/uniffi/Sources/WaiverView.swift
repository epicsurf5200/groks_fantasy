import SwiftUI

struct WaiverView: View {
    @EnvironmentObject var store: AppStore
    @State private var working = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                List {
                    if let r = store.waiver {
                        Section("Top candidates") {
                            ForEach(r.candidates, id: \.player.id) { c in WaiverRow(c: c) }
                        }
                        if !r.summary.isEmpty {
                            Section("Claude analysis") { Text(r.summary).font(.callout) }
                        }
                    } else {
                        Section { Text("No suggestions yet.") }
                    }
                }
                Divider()
                Button {
                    working = true
                    Task { await store.suggestWaivers(); working = false }
                } label: {
                    if working { ProgressView() } else {
                        Text("Suggest waiver pickups").bold().frame(maxWidth: .infinity)
                    }
                }
                .buttonStyle(.borderedProminent)
                .padding()
                .disabled(working)
            }
            .navigationTitle("Waiver")
            .padding(.top, 60)
        }
    }
}

struct WaiverRow: View {
    let c: UWaiverCandidate
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text("#\(c.priority)").font(.caption.monospaced()).frame(width: 30, alignment: .leading)
                Text(c.player.name).font(.body.bold())
                Text("\(c.player.position) · \(c.player.team)").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Text(String(format: "ROS %.0f", c.metrics.rosValue)).font(.callout.monospaced())
            }
            HStack(spacing: 12) {
                Tag("floor \(String(format: "%.1f", c.metrics.floor))")
                Tag("ceil \(String(format: "%.1f", c.metrics.ceiling))")
                Tag(String(format: "risk %.2f", c.metrics.riskScore))
                Tag(String(format: "fit %.2f", c.metrics.strategyFit))
            }
            if let drop = c.dropPlayerName {
                Text("drop \(drop) (Δ \(String(format: "%+.0f", c.dropRosDelta)))")
                    .font(.caption).foregroundStyle(.orange)
            }
        }
    }
}

private struct Tag: View {
    let text: String
    init(_ t: String) { self.text = t }
    var body: some View {
        Text(text)
            .font(.caption2.monospaced())
            .padding(.horizontal, 6).padding(.vertical, 2)
            .background(Color.secondary.opacity(0.15))
            .clipShape(Capsule())
    }
}
