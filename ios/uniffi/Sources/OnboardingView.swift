import SwiftUI
import UniformTypeIdentifiers

struct OnboardingView: View {
    @EnvironmentObject var store: AppStore
    @State private var configText: String = ""
    @State private var apiKey: String = ""
    @State private var importing = false

    var body: some View {
        NavigationStack {
            Form {
                Section("config.yaml") {
                    TextEditor(text: $configText)
                        .font(.system(.footnote, design: .monospaced))
                        .frame(minHeight: 220)
                    Button("Import from Files…") { importing = true }
                }
                Section("Anthropic API key (optional override)") {
                    SecureField("sk-ant-…", text: $apiKey)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                }
                Section {
                    Button {
                        store.initialize(configYaml: configText, apiKeyOverride: apiKey.isEmpty ? nil : apiKey)
                    } label: {
                        Text("Connect").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(configText.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                Section("Notes") {
                    Text(
                        "Your config and API key live in the iOS keychain on this device. The Rust core inside the app talks directly to Anthropic and your fantasy provider — there is no intermediate server."
                    )
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("groks_fantasy")
            .fileImporter(
                isPresented: $importing,
                allowedContentTypes: [.yaml, .text, .data],
                allowsMultipleSelection: false
            ) { result in
                if case let .success(urls) = result, let url = urls.first {
                    if url.startAccessingSecurityScopedResource() {
                        defer { url.stopAccessingSecurityScopedResource() }
                        if let s = try? String(contentsOf: url, encoding: .utf8) {
                            configText = s
                        }
                    }
                }
            }
        }
    }
}

extension UTType {
    static let yaml = UTType("public.yaml") ?? .text
}
