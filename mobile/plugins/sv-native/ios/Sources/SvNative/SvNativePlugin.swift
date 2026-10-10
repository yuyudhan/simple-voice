// FilePath: mobile/plugins/sv-native/ios/Sources/SvNative/SvNativePlugin.swift
// Tauri's entry into the Swift side. Rust calls `listen` once with a channel, then `reply` for
// every answer and `refresh` after settings change. Tauri runs these on its IPC queue; the
// session lives on the main thread, so each hops there.

import Foundation
import SwiftRs
import Tauri
import UIKit
import WebKit

struct ListenArgs: Decodable {
    let channel: Channel
}

struct ReplyArgs: Decodable {
    let id: UInt64
    /// JSON text, decoded by whoever asked.
    let body: String
}

final class SvNativePlugin: Plugin {
    private var session: SessionController?

    @objc func listen(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(ListenArgs.self)
        invoke.resolve()
        DispatchQueue.main.async {
            let session = SessionController(core: RustCore(channel: args.channel))
            self.session = session
            session.start()
        }
    }

    @objc func reply(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(ReplyArgs.self)
        invoke.resolve()
        DispatchQueue.main.async {
            self.session?.core.receive(id: args.id, body: Data(args.body.utf8))
        }
    }

    @objc func refresh(_ invoke: Invoke) {
        invoke.resolve()
        DispatchQueue.main.async { self.session?.refreshStatus() }
    }
}

@_cdecl("init_plugin_sv_native")
func initPlugin() -> Plugin {
    SvNativePlugin()
}

/// Requests to Rust and their answers, matched by id. Main thread only.
final class RustCore {
    private let channel: Channel
    private var nextId: UInt64 = 1
    private var pending: [UInt64: (Data) -> Void] = [:]

    init(channel: Channel) {
        self.channel = channel
    }

    /// Sends `request`; `done` gets the decoded answer, or nil when Rust's reply does not decode.
    func ask<Answer: Decodable>(_ request: NativeAsk, as type: Answer.Type, done: @escaping (Answer?) -> Void) {
        let id = nextId
        nextId += 1
        pending[id] = { data in done(try? JSONDecoder().decode(type, from: data)) }
        do {
            try channel.send(Envelope(id: id, request: request))
        } catch {
            pending.removeValue(forKey: id)
            done(nil)
        }
    }

    func receive(id: UInt64, body: Data) {
        pending.removeValue(forKey: id)?(body)
    }
}

/// What Swift asks Rust; mirrors `NativeRequest` in the plugin's Rust code.
enum NativeAsk {
    case status(online: Bool, onDeviceSpeech: Bool)
    case dictate(wavPath: String, durationMs: Int64)
    case local(wavPath: String, durationMs: Int64, text: String, language: String?, transcribeMs: Int64)
    case retry(historyId: Int64)
    case notInserted(historyId: Int64)
}

private struct Envelope: Encodable {
    let id: UInt64
    let request: NativeAsk

    enum Key: String, CodingKey {
        case id, kind, online, onDeviceSpeech, wavPath, durationMs, text, language, transcribeMs, historyId
    }

    func encode(to encoder: Encoder) throws {
        var out = encoder.container(keyedBy: Key.self)
        try out.encode(id, forKey: .id)
        switch request {
        case let .status(online, onDeviceSpeech):
            try out.encode("status", forKey: .kind)
            try out.encode(online, forKey: .online)
            try out.encode(onDeviceSpeech, forKey: .onDeviceSpeech)
        case let .dictate(wavPath, durationMs):
            try out.encode("dictate", forKey: .kind)
            try out.encode(wavPath, forKey: .wavPath)
            try out.encode(durationMs, forKey: .durationMs)
        case let .local(wavPath, durationMs, text, language, transcribeMs):
            try out.encode("local", forKey: .kind)
            try out.encode(wavPath, forKey: .wavPath)
            try out.encode(durationMs, forKey: .durationMs)
            try out.encode(text, forKey: .text)
            try out.encode(language, forKey: .language)
            try out.encode(transcribeMs, forKey: .transcribeMs)
        case let .retry(historyId):
            try out.encode("retry", forKey: .kind)
            try out.encode(historyId, forKey: .historyId)
        case let .notInserted(historyId):
            try out.encode("notInserted", forKey: .kind)
            try out.encode(historyId, forKey: .historyId)
        }
    }
}

/// Rust's answer to `status`.
struct NativeStatus: Decodable {
    let line: String
    let ready: Bool
    let needsKey: Bool
    let maxSeconds: Int
    let sessionMinutes: Int
}

/// Rust's `Dictation`: `text`, `noSpeech` or `failed`.
struct Dictation: Decodable {
    let kind: String
    let id: Int64?
    let text: String?
    let message: String?
}
