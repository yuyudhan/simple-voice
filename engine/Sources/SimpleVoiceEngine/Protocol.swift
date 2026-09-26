// FilePath: engine/Sources/SimpleVoiceEngine/Protocol.swift
// JSON-lines wire format shared with the Rust `sv-engine` crate (docs/internal/architecture.md § 6).

import Foundation

let engineVersion = "0.1.0"

/// An error whose message is sent to the app verbatim.
struct EngineError: LocalizedError, CustomStringConvertible {
    let message: String

    init(_ message: String) {
        self.message = message
    }

    var errorDescription: String? { message }
    var description: String { message }
}

/// Turns any thrown error into the single-line message the app shows to the user.
func describe(_ error: any Error) -> String {
    if let engineError = error as? EngineError {
        return engineError.message
    }
    if let localized = error as? any LocalizedError, let text = localized.errorDescription, !text.isEmpty {
        return text
    }
    // Cocoa errors (URL loading, file system, Core Audio) carry their own readable description;
    // plain Swift errors do not, and their case name is more useful than the generic bridge text.
    let bridged = error as NSError
    if let text = bridged.userInfo[NSLocalizedDescriptionKey] as? String, !text.isEmpty {
        return text
    }
    return String(describing: error)
}

enum Log {
    static func info(_ message: String) {
        write("info", message)
    }

    static func error(_ message: String) {
        write("error", message)
    }

    private static func write(_ level: String, _ message: String) {
        let line = "simple-voice-engine [\(level)] \(message)\n"
        FileHandle.standardError.write(Data(line.utf8))
    }
}

// MARK: - Requests

struct Request: Sendable {
    let id: Int64
    let cmd: String
    let body: Data

    func params<T: Decodable>(_ type: T.Type) throws -> T {
        do {
            return try JSONDecoder().decode(T.self, from: body)
        } catch DecodingError.keyNotFound(let key, _) {
            throw EngineError("\(cmd): missing parameter `\(key.stringValue)`")
        } catch DecodingError.typeMismatch(_, let context) {
            throw EngineError("\(cmd): wrong type for `\(Self.path(context))`")
        } catch DecodingError.valueNotFound(_, let context) {
            throw EngineError("\(cmd): null value for `\(Self.path(context))`")
        } catch {
            throw EngineError("\(cmd): invalid parameters (\(describe(error)))")
        }
    }

    private static func path(_ context: DecodingError.Context) -> String {
        context.codingPath.map(\.stringValue).joined(separator: ".")
    }
}

/// Only the routing fields; each command decodes its own parameters from the same line.
struct RequestEnvelope: Decodable {
    let id: Int64?
    let cmd: String?
}

struct ModelParams: Decodable {
    let model: String
    let language: String?
}

struct TranscribeParams: Decodable {
    let model: String
    let wavPath: String
    let language: String?
}

struct KindParams: Decodable {
    let kind: String
}

struct MuteParams: Decodable {
    let muted: Bool
}

struct PolishShot: Decodable, Sendable {
    let user: String
    let assistant: String
}

struct PolishParams: Decodable, Sendable {
    let system: String
    let shots: [PolishShot]
    let user: String
}

enum ModelID: String, Sendable, CaseIterable {
    case parakeetTdtV3 = "parakeet-tdt-v3"
    case parakeetTdtV2 = "parakeet-tdt-v2"
    case parakeetFlash = "parakeet-flash"
    case appleSpeech = "apple-speech"
    case appleIntelligence = "apple-intelligence"

    static func parse(_ raw: String) throws -> ModelID {
        guard let id = ModelID(rawValue: raw) else {
            throw EngineError("unknown model `\(raw)`")
        }
        return id
    }
}

enum PermissionKind: String, Sendable {
    case microphone
    case accessibility
    case speech

    static func parse(_ raw: String) throws -> PermissionKind {
        guard let kind = PermissionKind(rawValue: raw) else {
            throw EngineError("unknown permission kind `\(raw)`")
        }
        return kind
    }
}

enum LanguageCode {
    /// The lowercased primary subtag of a language code or locale (`en-GB` → `en`), or nil when
    /// the app did not send one.
    static func primary(_ language: String?) -> String? {
        guard let language else { return nil }
        let trimmed = language.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let first = trimmed.split(whereSeparator: { $0 == "-" || $0 == "_" }).first else { return nil }
        return first.lowercased()
    }
}

// MARK: - Results

typealias CommandResult = any Encodable & Sendable

struct VersionResult: Encodable, Sendable {
    let version: String
}

struct EmptyResult: Encodable, Sendable {}

enum ModelAvailability: String, Encodable, Sendable {
    case ready
    case notDownloaded = "not_downloaded"
    case unsupported
}

struct ModelStatusResult: Encodable, Sendable {
    let status: ModelAvailability
    let sizeBytes: Int64?
    let reason: String?

    static func ready(sizeBytes: Int64? = nil) -> ModelStatusResult {
        ModelStatusResult(status: .ready, sizeBytes: sizeBytes, reason: nil)
    }

    static func notDownloaded(reason: String? = nil) -> ModelStatusResult {
        ModelStatusResult(status: .notDownloaded, sizeBytes: nil, reason: reason)
    }

    static func unsupported(_ reason: String) -> ModelStatusResult {
        ModelStatusResult(status: .unsupported, sizeBytes: nil, reason: reason)
    }
}

struct DownloadResult: Encodable, Sendable {
    let status = ModelAvailability.ready
}

struct TranscriptResult: Encodable, Sendable {
    let text: String
    let language: String?
}

enum PermissionState: String, Encodable, Sendable {
    case granted
    case denied
    case notDetermined = "not_determined"
    case restricted
}

struct PermissionsResult: Encodable, Sendable {
    let microphone: PermissionState
    let accessibility: PermissionState
    let speech: PermissionState
}

struct FrontmostAppResult: Encodable, Sendable {
    let name: String?
    let bundleId: String?
}

/// `text` is absent when nothing is selected.
struct SelectedTextResult: Encodable, Sendable {
    let text: String?
}

struct MutedResult: Encodable, Sendable {
    let previous: Bool
}

struct PolishResult: Encodable, Sendable {
    let text: String
    let finished: Bool
}

// MARK: - Output

private struct SuccessLine: Encodable {
    let id: Int64
    let ok = true
    let result: CommandResult

    enum CodingKeys: String, CodingKey {
        case id
        case ok
        case result
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(ok, forKey: .ok)
        try container.encode(result, forKey: .result)
    }
}

private struct FailureLine: Encodable {
    let id: Int64
    let ok = false
    let error: String
}

private struct ProgressLine: Encodable {
    let id: Int64
    let event = "progress"
    let fraction: Double
    let message: String
}

/// Unsolicited, so it carries no request id.
private struct FnKeyLine: Encodable {
    let event = "fn_key"
    let action: FnKeyAction
}

/// Serializes every stdout line so concurrent requests never interleave bytes.
actor Output {
    private let handle: FileHandle
    private let encoder: JSONEncoder

    init(handle: FileHandle) {
        self.handle = handle
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        self.encoder = encoder
    }

    func success(id: Int64, result: CommandResult) {
        emit(SuccessLine(id: id, result: result))
    }

    func failure(id: Int64, message: String) {
        emit(FailureLine(id: id, error: message))
    }

    func progress(id: Int64, fraction: Double, message: String) {
        emit(ProgressLine(id: id, fraction: min(max(fraction, 0), 1), message: message))
    }

    func fnKey(_ action: FnKeyAction) {
        emit(FnKeyLine(action: action))
    }

    private func emit(_ line: some Encodable) {
        do {
            var data = try encoder.encode(line)
            data.append(0x0A)
            // A single write per line; FileHandle is unbuffered, so this is also the flush.
            try handle.write(contentsOf: data)
        } catch {
            Log.error("failed to write response: \(describe(error))")
        }
    }
}

/// Progress events for one request, rate limited to at most 10 per second. Fractions never go
/// backwards, so late-arriving reports from concurrent callbacks are dropped.
actor ProgressSink {
    private let id: Int64
    private let output: Output
    private var lastEmit: ContinuousClock.Instant?
    private var lastFraction = -1.0

    init(id: Int64, output: Output) {
        self.id = id
        self.output = output
    }

    func report(_ fraction: Double, _ message: String, force: Bool = false) async {
        guard fraction >= lastFraction else { return }
        let now = ContinuousClock.now
        if !force, let lastEmit, now - lastEmit < .milliseconds(100) {
            return
        }
        lastEmit = now
        lastFraction = fraction
        await output.progress(id: id, fraction: fraction, message: message)
    }
}
