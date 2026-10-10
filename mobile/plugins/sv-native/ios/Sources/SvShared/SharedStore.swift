// FilePath: mobile/plugins/sv-native/ios/Sources/SvShared/SharedStore.swift
// Where the keyboard and the app keep what they share. The App Group folder is the natural
// place, but a free Apple ID (Personal Team) cannot provision App Groups; it can share a
// keychain access group, so that is the fallback. Both stores hold small JSON values by name.

import Foundation
import Security

protocol SharedStore {
    func read(_ name: String) -> Data?
    func write(_ name: String, _ data: Data)
    func remove(_ name: String)
    /// Names starting with `prefix`, in name order.
    func names(prefix: String) -> [String]
}

/// Files in the App Group container (`SimpleVoice/`), written atomically.
struct AppGroupStore: SharedStore {
    let root: URL

    init?() {
        let manager = FileManager.default
        guard let container = manager.containerURL(forSecurityApplicationGroupIdentifier: Bridge.appGroup)
        else { return nil }
        root = container.appendingPathComponent("SimpleVoice", isDirectory: true)
        try? manager.createDirectory(at: root, withIntermediateDirectories: true)
    }

    func read(_ name: String) -> Data? { try? Data(contentsOf: root.appendingPathComponent(name)) }

    func write(_ name: String, _ data: Data) {
        try? data.write(to: root.appendingPathComponent(name), options: .atomic)
    }

    func remove(_ name: String) { try? FileManager.default.removeItem(at: root.appendingPathComponent(name)) }

    func names(prefix: String) -> [String] {
        let all = (try? FileManager.default.contentsOfDirectory(atPath: root.path)) ?? []
        return all.filter { $0.hasPrefix(prefix) }.sorted()
    }
}

/// Generic-password items in the shared keychain access group named by the `SVKeychainGroup`
/// Info.plist key (`$(AppIdentifierPrefix)dev.yuyudhan.simplevoice.shared`). A keyboard reaches
/// it only with Full Access; without it, `init` fails and the keyboard says so.
struct KeychainStore: SharedStore {
    private static let service = "dev.yuyudhan.simplevoice.bridge"
    private let group: String

    init?() {
        guard let group = Bundle.main.object(forInfoDictionaryKey: "SVKeychainGroup") as? String,
              !group.isEmpty, !group.hasPrefix("$(")
        else { return nil }
        self.group = group
        // A write is the only proof of access: reads of a missing item look the same either way.
        write("probe", Data([1]))
        guard read("probe") != nil else { return nil }
    }

    private func base(_ name: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccessGroup as String: group,
            kSecAttrAccount as String: name,
        ]
    }

    func read(_ name: String) -> Data? {
        var query = base(name)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess else { return nil }
        return result as? Data
    }

    func write(_ name: String, _ data: Data) {
        let update = [kSecValueData as String: data] as CFDictionary
        if SecItemUpdate(base(name) as CFDictionary, update) == errSecItemNotFound {
            var add = base(name)
            add[kSecValueData as String] = data
            // The app reads requests while the phone is locked in a pocket.
            add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
            SecItemAdd(add as CFDictionary, nil)
        }
    }

    func remove(_ name: String) { SecItemDelete(base(name) as CFDictionary) }

    func names(prefix: String) -> [String] {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccessGroup as String: group,
            kSecReturnAttributes as String: true,
            kSecMatchLimit as String: kSecMatchLimitAll,
        ]
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let items = result as? [[String: Any]]
        else { return [] }
        return items.compactMap { $0[kSecAttrAccount as String] as? String }
            .filter { $0.hasPrefix(prefix) }
            .sorted()
    }
}
