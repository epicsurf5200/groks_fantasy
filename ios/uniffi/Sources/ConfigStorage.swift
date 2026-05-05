import Foundation
import Security

/// Stores the config YAML and (optional) Anthropic API key in the keychain so
/// they aren't included in iCloud backups in plaintext.
enum ConfigStorage {
    private static let service = "com.groksfantasy.config"
    private static let yamlKey = "config.yaml"
    private static let apiKeyKey = "anthropic.api_key"

    struct Saved { let yaml: String; let apiKey: String? }

    static func save(yaml: String, apiKey: String?) {
        write(account: yamlKey, value: yaml)
        if let apiKey, !apiKey.isEmpty { write(account: apiKeyKey, value: apiKey) }
    }

    static func load() -> Saved? {
        guard let yaml = read(account: yamlKey) else { return nil }
        return Saved(yaml: yaml, apiKey: read(account: apiKeyKey))
    }

    static func clear() {
        delete(account: yamlKey)
        delete(account: apiKeyKey)
    }

    private static func write(account: String, value: String) {
        let data = Data(value.utf8)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        SecItemDelete(query as CFDictionary)
        var attrs = query
        attrs[kSecValueData as String] = data
        attrs[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
        SecItemAdd(attrs as CFDictionary, nil)
    }

    private static func read(account: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private static func delete(account: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        SecItemDelete(query as CFDictionary)
    }
}
