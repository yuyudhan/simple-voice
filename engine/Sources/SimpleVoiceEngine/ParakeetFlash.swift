// FilePath: engine/Sources/SimpleVoiceEngine/ParakeetFlash.swift
// Parakeet Flash (Parakeet realtime EOU 120M) used for whole recordings and live dictation.

import AVFoundation
import CoreML
import FluidAudio
import Foundation

struct FlashDecoded: Sendable {
    let text: String
    let words: [TimedWord]
}

/// Parakeet Flash is a streaming model. For a finished recording the whole file is fed through
/// the streaming manager and then flushed, which yields the text a live session would produce.
actor FlashTranscriber {
    private let manager: StreamingEouAsrManager

    private init(manager: StreamingEouAsrManager) {
        self.manager = manager
    }

    /// Loads the 1280 ms chunk variant from a directory laid out like the Hugging Face folder.
    static func load(from directory: URL) async throws -> FlashTranscriber {
        FlashTranscriber(manager: try await makeManager(from: directory))
    }

    static func makeManager(from directory: URL) async throws -> StreamingEouAsrManager {
        let configuration = MLModelConfiguration()
        configuration.computeUnits = .cpuAndNeuralEngine
        let manager = StreamingEouAsrManager(configuration: configuration, chunkSize: .ms1280)
        try await manager.loadModels(from: directory)
        return manager
    }

    func transcribe(_ samples: [Float]) async throws -> FlashDecoded {
        guard !samples.isEmpty else { return FlashDecoded(text: "", words: []) }
        // Every recording is its own session; state from a previous (possibly failed) run must
        // not leak into this one.
        await manager.reset()
        let buffer = try AudioLoader.makeBuffer(samples)
        _ = try await manager.process(audioBuffer: buffer)
        // `finish` clears the token timings, so the tail is padded to a whole chunk and decoded
        // first; the timings are complete once every real sample has been through the encoder.
        await manager.injectSilence(Double(manager.chunkSize.chunkSamples) / AudioLoader.sampleRate)
        try await manager.processBufferedAudio()
        let words = await Self.words(of: manager)
        let text = try await manager.finish()
        await manager.reset()
        return FlashDecoded(text: text.trimmingCharacters(in: .whitespacesAndNewlines), words: words)
    }

    func cleanup() async {
        await manager.cleanup()
    }

    /// Word timings from the manager's accumulated SentencePiece tokens; a token lasts until the
    /// next one starts (the model reports start times only).
    static func words(of manager: StreamingEouAsrManager) async -> [TimedWord] {
        let starts = await manager.getTokenTimestampsMs()
        let pieces = await manager.getRawTokenStrings()
        let count = min(starts.count, pieces.count)
        guard count > 0 else { return [] }
        let tokenDuration = 0.08
        var timings: [TokenTiming] = []
        timings.reserveCapacity(count)
        for index in 0..<count {
            let start = Double(starts[index]) / 1000
            let next = index + 1 < count ? Double(starts[index + 1]) / 1000 : start + tokenDuration
            timings.append(
                TokenTiming(
                    token: pieces[index], tokenId: index, startTime: start, endTime: max(start, min(next, start + 1)),
                    confidence: 1))
        }
        return buildWordTimings(from: timings).map { TimedWord(text: $0.word, start: $0.startTime, end: $0.endTime) }
    }
}
