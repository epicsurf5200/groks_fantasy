import SwiftUI

struct TradeView: View {
    @EnvironmentObject var store: AppStore
    @State private var partner = ""
    @State private var send = ""
    @State private var receive = ""
    @State private var working = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Proposal") {
                    TextField("Partner team", text: $partner)
                        .autocorrectionDisabled()
                    TextField("You send (comma-separated)", text: $send)
                        .autocorrectionDisabled()
                    TextField("You receive (comma-separated)", text: $receive)
                        .autocorrectionDisabled()
                    Button {
                        working = true
                        Task {
                            await store.analyzeTrade(partner: partner, send: send, receive: receive)
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
                if let a = store.trade {
                    Section("Verdict") {
                        HStack {
                            Text(a.verdict)
                                .font(.title3.bold())
                                .foregroundStyle(verdictColor(a.verdict))
                            Spacer()
                            Text(String(format: "net ROS %+.1f", a.netRosDelta))
                                .font(.callout.monospaced())
                        }
                        Text(String(format: "fairness %.0f%%", a.fairness * 100))
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    Section("You send") {
                        ForEach(Array(a.sendNames.enumerated()), id: \.offset) { i, n in
                            MetricsRow(name: n, m: a.send[i])
                        }
                        Text(String(format: "Total ROS %.0f · ceil %.1f", a.sendTotalRos, a.sendTotalCeiling))
                            .font(.caption.monospaced()).foregroundStyle(.secondary)
                    }
                    Section("You receive") {
                        ForEach(Array(a.receiveNames.enumerated()), id: \.offset) { i, n in
                            MetricsRow(name: n, m: a.receive[i])
                        }
                        Text(String(format: "Total ROS %.0f · ceil %.1f", a.receiveTotalRos, a.receiveTotalCeiling))
                            .font(.caption.monospaced()).foregroundStyle(.secondary)
                    }
                    if !a.summary.isEmpty {
                        Section("Claude analysis") {
                            Text(a.summary).font(.callout)
                        }
                    }
                }
            }
            .navigationTitle("Trade")
            .padding(.top, 60)
        }
    }

    private func verdictColor(_ v: String) -> Color {
        switch v {
        case "ACCEPT": return .green
        case "DECLINE": return .red
        default: return .yellow
        }
    }
}

private struct MetricsRow: View {
    let name: String
    let m: UMetrics
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(name).font(.body)
            Text(String(format: "mean %.1f · floor %.1f · ceil %.1f · ROS %.0f · risk %.2f",
                         m.meanProjection, m.floor, m.ceiling, m.rosValue, m.riskScore))
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
        }
    }
}
