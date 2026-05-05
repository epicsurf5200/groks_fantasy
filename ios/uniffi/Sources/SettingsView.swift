import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        NavigationStack {
            Form {
                Section("Strategy") {
                    Picker("Mode", selection: Binding(
                        get: { store.strategyLabel },
                        set: { newLabel in store.setStrategy(strategyKey(for: newLabel)) }
                    )) {
                        Text("Conservative").tag("Conservative")
                        Text("Balanced").tag("Balanced")
                        Text("High Stakes / High Rewards").tag("High Stakes / High Rewards")
                    }
                }
                Section("Connection") {
                    Button("Refresh now") { store.refreshNow() }
                    Button("Forget config & reset", role: .destructive) {
                        ConfigStorage.clear()
                        store.hasClient = false
                    }
                }
                Section("About") {
                    Text("groks_fantasy — Rust core embedded via UniFFI.").font(.footnote)
                    Text("Anthropic + your fantasy provider are called directly from this device.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Settings")
            .padding(.top, 60)
        }
    }
}

private func strategyKey(for label: String) -> String {
    switch label {
    case "Conservative": return "conservative"
    case "Balanced": return "balanced"
    default: return "high_stakes"
    }
}
