// FilePath: engine/Sources/SimpleVoiceEngine/ParakeetFlash.swift
// Parakeet Flash (Parakeet realtime EOU 120M) used for whole recordings.

import AVFoundation
import CoreML
import FluidAudio
import Foundation

/// Parakeet Flash is a streaming model. For a finished recording the whole file is fed through
/// the streaming manager and then flushed, which yields the text a live session would produce.
actor FlashTranscriber {
    private let manager: StreamingEouAsrManager

    private init(manager: StreamingEouAsrManager) {
        self.manager = manager
    }

    /// Loads the 1280 ms chunk variant from a directory laid out like the Hugging Face folder.
    static func load(from directory: URL) async throws -> FlashTranscriber {
        let configuration = MLModelConfiguration()
        configuration.computeUnits = .cpuAndNeuralEngine
        let manager = StreamingEouAsrManager(configuration: configuration, chunkSize: .ms1280)
        try await manager.loadModels(from: directory)
        return FlashTranscriber(manager: manager)
    }

    func transcribe(_ samples: [Float]) async throws -> String {
        guard !samples.isEmpty else { return "" }
        // Every recording is its own session; state from a previous (possibly failed) run must
        // not leak into this one.
        await manager.reset()
        let buffer = try AudioLoader.makeBuffer(samples)
        _ = try await manager.process(audioBuffer: buffer)
        let text = try await manager.finish()
        await manager.reset()
        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    func cleanup() async {
        await manager.cleanup()
    }
}
