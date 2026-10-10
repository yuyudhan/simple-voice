// FilePath: mobile/plugins/sv-native/ios/Sources/SvShared/Bridge.swift
// What the iPhone keyboard and the app share through their App Group: a status file the app
// keeps current, a queue of keyboard requests, the app's latest reply and the microphone level.
// Each side writes, then posts a Darwin notification so the other reads at once. Compiled into
// the sv-native plugin (module SvShared) and directly into the keyboard and widget targets.

import Foundation

public enum Bridge {
    public static let appGroup = "group.dev.yuyudhan.simplevoice.mobile"
    /// The keyboard opens the app with this scheme to start a session.
    public static let sessionURL = URL(string: "simplevoice://session")
    public static let settingsURL = URL(string: "simplevoice://settings")

    public enum Note {
        public static let request = "dev.yuyudhan.simplevoice.request"
        public static let reply = "dev.yuyudhan.simplevoice.reply"
        public static let status = "dev.yuyudhan.simplevoice.status"
        public static let endSession = "dev.yuyudhan.simplevoice.endSession"
    }

    static let levelKey = "level"
    /// A session whose heartbeat is older than this is gone (the app was closed or killed).
    public static let aliveWindow: TimeInterval = 15
}

/// What the keyboard's top bar shows, written by the app whenever it changes.
public struct SharedStatus: Codable, Equatable {
    public var line: String
    public var ready: Bool
    public var needsKey: Bool
    public var maxSeconds: Int
    public var session: Session?
    public var recording: Recording?
    public var updatedAt: Double

    public struct Session: Codable, Equatable {
        /// Milliseconds since 1970.
        public var endsAt: Double
        public var aliveAt: Double

        public init(endsAt: Double, aliveAt: Double) {
            self.endsAt = endsAt
            self.aliveAt = aliveAt
        }
    }

    public struct Recording: Codable, Equatable {
        public var startedAt: Double

        public init(startedAt: Double) {
            self.startedAt = startedAt
        }
    }

    public init(
        line: String, ready: Bool, needsKey: Bool, maxSeconds: Int, session: Session?,
        recording: Recording?, updatedAt: Double
    ) {
        self.line = line
        self.ready = ready
        self.needsKey = needsKey
        self.maxSeconds = maxSeconds
        self.session = session
        self.recording = recording
        self.updatedAt = updatedAt
    }

    /// The app is running a session the keyboard can record through right now.
    public func sessionIsLive(now: Date = Date()) -> Bool {
        guard let session else { return false }
        let nowMs = now.timeIntervalSince1970 * 1000
        return session.endsAt > nowMs && nowMs - session.aliveAt < Bridge.aliveWindow * 1000
    }
}

public enum RequestKind: String, Codable {
    case startSession, start, stop, cancel, retry, ack
}

/// One keyboard request, queued as its own file so quick taps never overwrite each other.
public struct KeyboardRequest: Codable, Equatable {
    public var id: String
    public var kind: RequestKind
    public var historyId: Int64?
    public var at: Double

    public init(kind: RequestKind, historyId: Int64? = nil) {
        id = UUID().uuidString
        self.kind = kind
        self.historyId = historyId
        at = Date().timeIntervalSince1970 * 1000
    }
}

public enum ReplyKind: String, Codable {
    case recording, text, noSpeech, failed, error, sessionEnded
}

/// The app's answer to the latest request. `failed` carries the history row to retry.
public struct KeyboardReply: Codable, Equatable {
    public var requestId: String
    public var kind: ReplyKind
    public var text: String?
    public var historyId: Int64?
    public var message: String?
    public var at: Double

    public init(
        requestId: String, kind: ReplyKind, text: String? = nil, historyId: Int64? = nil,
        message: String? = nil
    ) {
        self.requestId = requestId
        self.kind = kind
        self.text = text
        self.historyId = historyId
        self.message = message
        at = Date().timeIntervalSince1970 * 1000
    }
}

/// The App Group folder. `nil` when the App Group is missing (no entitlement, or a keyboard
/// without Full Access); callers then report that instead of failing silently.
public final class SharedFiles {
    public let root: URL
    private let defaults: UserDefaults?

    public init?() {
        let manager = FileManager.default
        guard let container = manager.containerURL(forSecurityApplicationGroupIdentifier: Bridge.appGroup)
        else { return nil }
        root = container.appendingPathComponent("SimpleVoice", isDirectory: true)
        defaults = UserDefaults(suiteName: Bridge.appGroup)
        try? manager.createDirectory(at: requestsDir, withIntermediateDirectories: true)
    }

    var statusFile: URL { root.appendingPathComponent("status.json") }
    var replyFile: URL { root.appendingPathComponent("reply.json") }
    var requestsDir: URL { root.appendingPathComponent("requests", isDirectory: true) }

    public func readStatus() -> SharedStatus? { read(SharedStatus.self, from: statusFile) }

    public func writeStatus(_ status: SharedStatus) {
        write(status, to: statusFile)
        DarwinNote.post(Bridge.Note.status)
    }

    public func readReply() -> KeyboardReply? { read(KeyboardReply.self, from: replyFile) }

    public func writeReply(_ reply: KeyboardReply) {
        write(reply, to: replyFile)
        DarwinNote.post(Bridge.Note.reply)
    }

    public func enqueue(_ request: KeyboardRequest) {
        let name = String(format: "%013.0f-%@.json", request.at, request.id)
        write(request, to: requestsDir.appendingPathComponent(name))
        DarwinNote.post(Bridge.Note.request)
    }

    /// Every queued request, oldest first; each is removed as it is read.
    public func takeRequests() -> [KeyboardRequest] {
        let manager = FileManager.default
        let files = (try? manager.contentsOfDirectory(at: requestsDir, includingPropertiesForKeys: nil)) ?? []
        return files
            .filter { $0.pathExtension == "json" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
            .compactMap { file in
                defer { try? manager.removeItem(at: file) }
                return read(KeyboardRequest.self, from: file)
            }
    }

    public var level: Float {
        get { defaults?.float(forKey: Bridge.levelKey) ?? 0 }
        set { defaults?.set(newValue, forKey: Bridge.levelKey) }
    }

    private func read<T: Decodable>(_ type: T.Type, from file: URL) -> T? {
        guard let data = try? Data(contentsOf: file) else { return nil }
        return try? JSONDecoder().decode(type, from: data)
    }

    private func write<T: Encodable>(_ value: T, to file: URL) {
        guard let data = try? JSONEncoder().encode(value) else { return }
        try? data.write(to: file, options: .atomic)
    }
}

/// Darwin notifications: the one signal that crosses from the keyboard's process to the app's.
/// They carry no payload; the receiver reads the App Group files.
public enum DarwinNote {
    private static var handlers: [String: () -> Void] = [:]
    // Darwin centers want a non-null observer token; this object only identifies us.
    private static let token = NSObject()

    public static func post(_ name: String) {
        let center = CFNotificationCenterGetDarwinNotifyCenter()
        CFNotificationCenterPostNotification(center, CFNotificationName(name as CFString), nil, nil, true)
    }

    /// Runs `handler` on the main queue each time `name` is posted. One handler per name.
    public static func observe(_ name: String, handler: @escaping () -> Void) {
        let isNew = handlers[name] == nil
        handlers[name] = handler
        guard isNew else { return }
        let center = CFNotificationCenterGetDarwinNotifyCenter()
        CFNotificationCenterAddObserver(
            center, Unmanaged.passUnretained(token).toOpaque(),
            { _, _, cfName, _, _ in
                guard let name = cfName?.rawValue as String? else { return }
                DispatchQueue.main.async { DarwinNote.handlers[name]?() }
            },
            name as CFString, nil, .deliverImmediately
        )
    }
}
