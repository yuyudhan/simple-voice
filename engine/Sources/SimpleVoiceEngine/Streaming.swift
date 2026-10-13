// FilePath: engine/Sources/SimpleVoiceEngine/Streaming.swift
// Live dictation: the app streams microphone audio while recording (`stream_audio`) and the
// helper recognises it incrementally, reporting the growing hypothesis as `stream_partial`
// events and, when asked, in the overlay pill. Only models that genuinely decode incrementally
// stream: Parakeet TDT (sliding windows), Parakeet Flash (cache-aware streaming encoder) and
// Apple Speech (volatile results). Whisper does not.

import AVFoundation
import CoreML
import FluidAudio
import Foundation

/// What a live recogniser currently believes was said. `stable` is the committed prefix the
/// model will not revise; `text` is `stable` plus the still-changing tail.
struct LiveHypothesis: Sendable, Equatable {
    var stable = ""
    var text = ""

    static func joining(stable: String, volatile: String) -> LiveHypothesis {
        let committed = stable.trimmingCharacters(in: .whitespacesAndNewlines)
        let tail = volatile.trimmingCharacters(in: .whitespacesAndNewlines)
        let text = [committed, tail].filter { !$0.isEmpty }.joined(separator: " ")
        return LiveHypothesis(stable: committed, text: text)
    }
}

/// One live recognition. Calls arrive in order from a single pump task.
protocol LiveRecognizer: Actor {
    func append(_ samples: [Float]) async throws
    func hypothesis() async -> LiveHypothesis
    /// Flushes the model and returns the final transcript; the recogniser is spent afterwards.
    func finish() async throws -> TranscriptResult
    func cancel() async
}

struct StreamStartParams: Decodable {
    let model: String
    let language: String?
    let overlay: Bool?
}

struct StreamIdParams: Decodable {
    let streamId: UInt64
}

struct StreamAudioParams: Decodable {
    let streamId: UInt64
    /// Base64 of 16 kHz mono signed 16-bit little-endian PCM.
    let pcm: String
}

struct StreamStartResult: Encodable, Sendable {
    let streamId: UInt64
}

private struct StreamPartialLine: Encodable, Sendable {
    let event = "stream_partial"
    let streamId: UInt64
    let text: String
    let stable: String
}

private struct StreamFailedLine: Encodable, Sendable {
    let event = "stream_failed"
    let streamId: UInt64
    let reason: String
}

/// The running live sessions. `feed` is called synchronously from the stdin reader so audio is
/// queued in arrival order; everything else is async.
final class LiveStreams: @unchecked Sendable {
    /// More unprocessed audio than this means the recogniser cannot keep up; the stream fails
    /// rather than growing without bound, and the app transcribes the recording afterwards.
    static let maxQueuedSamples = 30 * Int(AudioLoader.sampleRate)

    private let output: Output
    private let lock = NSLock()
    private var nextId: UInt64 = 0
    private var sessions: [UInt64: LiveSession] = [:]

    init(output: Output) {
        self.output = output
    }

    /// Ends every running session (a new dictation supersedes an unfinished one).
    func cancelAll() async {
        let previous = lock.withLock { () -> [LiveSession] in
            let running = Array(sessions.values)
            sessions.removeAll()
            return running
        }
        for session in previous {
            await session.cancel()
        }
    }

    /// Starts a session, ending every other one first: there is one live dictation at a time.
    func start(_ recognizer: any LiveRecognizer, overlay: Bool) async -> UInt64 {
        await cancelAll()
        let id = lock.withLock { () -> UInt64 in
            nextId += 1
            return nextId
        }
        let session = LiveSession(id: id, recognizer: recognizer, overlay: overlay, output: output)
        lock.withLock { sessions[id] = session }
        if overlay {
            await MainActor.run { OverlayController.shared.setLive("", stream: id) }
        }
        Log.info("live stream \(id) started")
        return id
    }

    /// Queues decoded audio for a session; unknown or finished ids are ignored.
    func feed(_ params: StreamAudioParams) {
        guard let session = lock.withLock({ sessions[params.streamId] }) else { return }
        guard let data = Data(base64Encoded: params.pcm), data.count >= 2 else { return }
        let count = data.count / 2
        var samples = [Float](repeating: 0, count: count)
        data.withUnsafeBytes { raw in
            for index in 0..<count {
                let value = Int16(littleEndian: raw.loadUnaligned(fromByteOffset: index * 2, as: Int16.self))
                samples[index] = Float(value) / 32_768
            }
        }
        session.enqueue(samples)
    }

    func finish(_ id: UInt64) async throws -> TranscriptResult {
        guard let session = lock.withLock({ sessions.removeValue(forKey: id) }) else {
            throw EngineError("live stream \(id) is not running")
        }
        return try await withTaskCancellationHandler {
            try await session.finish()
        } onCancel: {
            Task { await session.cancel() }
        }
    }

    func cancel(_ id: UInt64) async {
        guard let session = lock.withLock({ sessions.removeValue(forKey: id) }) else { return }
        await session.cancel()
    }
}

/// One stream: an ordered audio queue drained by a pump task into the recogniser.
private final class LiveSession: @unchecked Sendable {
    let id: UInt64
    private let recognizer: any LiveRecognizer
    private let overlay: Bool
    private let output: Output
    private let continuation: AsyncStream<[Float]>.Continuation
    private let lock = NSLock()
    private var queued = 0
    private var failure: String?
    private var last = LiveHypothesis()
    private var pump: Task<Void, Never>?
    /// Recognisers decode on their own tasks, so one poller (the only publisher, which keeps
    /// partials in order) reads the hypothesis on a short timer.
    private var poller: Task<Void, Never>?
    private static let pollInterval: Duration = .milliseconds(150)

    init(id: UInt64, recognizer: any LiveRecognizer, overlay: Bool, output: Output) {
        self.id = id
        self.recognizer = recognizer
        self.overlay = overlay
        self.output = output
        let (stream, continuation) = AsyncStream.makeStream(of: [Float].self)
        self.continuation = continuation
        pump = Task { [weak self] in
            for await chunk in stream {
                guard let self else { return }
                self.lock.withLock { self.queued -= chunk.count }
                do {
                    try await recognizer.append(chunk)
                } catch {
                    await self.fail("live transcription failed: \(describe(error))")
                    return
                }
            }
        }
        poller = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: Self.pollInterval)
                guard !Task.isCancelled, let self else { return }
                await self.refresh()
            }
        }
    }

    /// Publishes the hypothesis when it changed since the last publication.
    private func refresh() async {
        let current = await recognizer.hypothesis()
        let changed = lock.withLock { () -> Bool in
            guard failure == nil, current != last else { return false }
            last = current
            return true
        }
        if changed {
            await publish(current)
        }
    }

    func enqueue(_ samples: [Float]) {
        let overflow = lock.withLock { () -> Bool in
            guard failure == nil else { return false }
            if queued + samples.count > LiveStreams.maxQueuedSamples {
                return true
            }
            queued += samples.count
            return false
        }
        if overflow {
            Task { await fail("live transcription fell more than 30 s behind the recording") }
            return
        }
        continuation.yield(samples)
    }

    func finish() async throws -> TranscriptResult {
        continuation.finish()
        await pump?.value
        poller?.cancel()
        defer { clearOverlay() }
        if let failure = lock.withLock({ failure }) {
            await recognizer.cancel()
            throw EngineError(failure)
        }
        let result = try await recognizer.finish()
        Log.info("live stream \(id) finished")
        return result
    }

    func cancel() async {
        continuation.finish()
        pump?.cancel()
        poller?.cancel()
        await recognizer.cancel()
        clearOverlay()
        Log.info("live stream \(id) cancelled")
    }

    private func fail(_ reason: String) async {
        let first = lock.withLock { () -> Bool in
            guard failure == nil else { return false }
            failure = reason
            return true
        }
        guard first else { return }
        continuation.finish()
        Log.error("live stream \(id): \(reason)")
        await output.event(StreamFailedLine(streamId: id, reason: reason))
    }

    private func publish(_ hypothesis: LiveHypothesis) async {
        await output.event(StreamPartialLine(streamId: id, text: hypothesis.text, stable: hypothesis.stable))
        if overlay {
            let id = id
            await MainActor.run { OverlayController.shared.setLive(hypothesis.text, stream: id) }
        }
    }

    private func clearOverlay() {
        guard overlay else { return }
        let id = id
        Task { @MainActor in OverlayController.shared.clearLive(stream: id) }
    }
}

// MARK: - Parakeet TDT (sliding window)

/// FluidAudio's sliding-window recogniser over the resident TDT weights. FluidAudio 0.17.4
/// decodes only whole windows (its hypothesis chunk setting is unused), so the window centre is
/// kept short for live text: every 2 s of new audio is decoded with 10 s of left and 1 s of
/// right context (13 s, within the model's 15 s input). Text appears about 3 s behind speech.
actor SlidingWindowLive: LiveRecognizer {
    private let manager: SlidingWindowAsrManager
    private let language: String?

    private init(manager: SlidingWindowAsrManager, language: String?) {
        self.manager = manager
        self.language = language
    }

    static func start(models: AsrModels, version: AsrModelVersion, language: String?) async throws -> SlidingWindowLive
    {
        let code = LanguageCode.primary(language)
        var config = SlidingWindowAsrConfig(
            chunkSeconds: 2.0,
            hypothesisChunkSeconds: 1.0,
            leftContextSeconds: 10.0,
            rightContextSeconds: 1.0,
            minContextForConfirmation: 10.0,
            confirmationThreshold: 0.80,
            tdtConfig: TdtConfig(blankId: version.blankId)
        )
        if version == .v3, let hint = code.flatMap({ Language(rawValue: $0) }) {
            config = config.applying(language: hint)
        }
        let manager = SlidingWindowAsrManager(config: config)
        try await manager.loadModels(models)
        try await manager.startStreaming(source: .microphone)
        return SlidingWindowLive(manager: manager, language: version == .v2 ? "en" : code)
    }

    func append(_ samples: [Float]) async throws {
        let buffer = try AudioLoader.makeBuffer(samples)
        await manager.streamAudio(buffer)
    }

    func hypothesis() async -> LiveHypothesis {
        LiveHypothesis.joining(
            stable: await manager.confirmedTranscript, volatile: await manager.volatileTranscript)
    }

    func finish() async throws -> TranscriptResult {
        let text = try await manager.finish()
        return TranscriptResult(text: text.trimmingCharacters(in: .whitespacesAndNewlines), language: language)
    }

    func cancel() async {
        await manager.cancel()
    }
}

// MARK: - Parakeet Flash (streaming EOU model)

/// Parakeet realtime EOU 120M decodes each 1.28 s chunk as it arrives; text up to the model's
/// last end-of-utterance is committed.
actor FlashLive: LiveRecognizer {
    private let manager: StreamingEouAsrManager
    private let committed: CommittedText

    private init(manager: StreamingEouAsrManager, committed: CommittedText) {
        self.manager = manager
        self.committed = committed
    }

    static func start(manager: StreamingEouAsrManager) async throws -> FlashLive {
        await manager.reset()
        let committed = CommittedText()
        await manager.setEouCallback { transcript in committed.set(transcript) }
        return FlashLive(manager: manager, committed: committed)
    }

    func append(_ samples: [Float]) async throws {
        _ = try await manager.process(audioBuffer: try AudioLoader.makeBuffer(samples))
    }

    func hypothesis() async -> LiveHypothesis {
        let text = (await manager.getPartialTranscript()).trimmingCharacters(in: .whitespacesAndNewlines)
        let stable = committed.get().trimmingCharacters(in: .whitespacesAndNewlines)
        guard !stable.isEmpty, text.hasPrefix(stable) else { return LiveHypothesis(stable: "", text: text) }
        return LiveHypothesis(stable: stable, text: text)
    }

    func finish() async throws -> TranscriptResult {
        let text = try await manager.finish()
        await manager.reset()
        return TranscriptResult(text: text.trimmingCharacters(in: .whitespacesAndNewlines), language: "en")
    }

    func cancel() async {
        await manager.reset()
    }
}

/// The transcript the EOU callback last committed, written from FluidAudio's actor.
final class CommittedText: @unchecked Sendable {
    private let lock = NSLock()
    private var value = ""

    func set(_ text: String) {
        lock.withLock { value = text }
    }

    func get() -> String {
        lock.withLock { value }
    }
}

/// The streaming Parakeet Flash manager, loaded once and kept apart from the batch transcriber
/// so a live dictation and a file transcription never share decoder state.
actor FlashStreamingModel {
    private let store: ModelStore
    private var manager: StreamingEouAsrManager?

    init(store: ModelStore) {
        self.store = store
    }

    func load() async throws -> StreamingEouAsrManager {
        if let manager { return manager }
        guard let hub = HubModel.forModel(.parakeetFlash) else { throw EngineError("parakeet-flash has no files") }
        let directory = try store.readyDirectory(for: hub)
        let loaded = try await FlashTranscriber.makeManager(from: directory)
        manager = loaded
        return loaded
    }

    func unload() async {
        guard let manager else { return }
        self.manager = nil
        await manager.cleanup()
    }
}
