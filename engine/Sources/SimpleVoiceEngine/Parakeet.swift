// FilePath: engine/Sources/SimpleVoiceEngine/Parakeet.swift
// Parakeet TDT v3 / v2 batch transcription through FluidAudio, with models kept resident.

import FluidAudio
import Foundation

/// Loaded Parakeet models, one per model id, kept in memory so a dictation only pays for
/// inference. Transcriptions run one at a time: the models share the Neural Engine anyway, and
/// the streaming manager behind Parakeet Flash holds per-session state.
actor ParakeetEngine {
    private enum Loaded: Sendable {
        case tdt(AsrManager, AsrModelVersion, AsrModels)
        case flash(FlashTranscriber)
    }

    private let store: ModelStore
    private let gate = SerialGate()
    private var loaded: [ModelID: Loaded] = [:]
    private var loading: [ModelID: Task<Loaded, any Error>] = [:]

    init(store: ModelStore) {
        self.store = store
    }

    func preload(_ id: ModelID) async throws {
        _ = try await model(for: id)
    }

    func unload(_ id: ModelID) async {
        loading.removeValue(forKey: id)?.cancel()
        guard let model = loaded.removeValue(forKey: id) else { return }
        switch model {
        case .tdt(let manager, _, _):
            await manager.cleanup()
        case .flash(let flash):
            await flash.cleanup()
        }
        Log.info("unloaded \(id.rawValue)")
    }

    /// The resident TDT models, shared with the sliding-window streaming recogniser so a live
    /// dictation does not load the weights a second time.
    func tdtModels(_ id: ModelID) async throws -> (AsrModels, AsrModelVersion) {
        switch try await model(for: id) {
        case .tdt(_, let version, let models):
            return (models, version)
        case .flash:
            throw EngineError("\(id.rawValue) is not a Parakeet TDT model")
        }
    }

    func transcribe(
        _ id: ModelID,
        samples: [Float],
        language: String?,
        timestamps: Bool
    ) async throws -> TranscriptResult {
        let model = try await model(for: id)
        let code = LanguageCode.primary(language)
        switch model {
        case .tdt(let manager, let version, _):
            let decoded = try await gate.run {
                try await Self.transcribeTdt(manager: manager, version: version, samples: samples, code: code)
            }
            // v2 is English only; v3 detects the language itself and only reports what was asked.
            return TranscriptResult(
                text: decoded.text,
                language: version == .v2 ? "en" : code,
                quality: decoded.confidence.map { TranscriptQuality(confidence: $0) },
                segments: timestamps ? Timeline.segments(from: decoded.words) : nil
            )
        case .flash(let flash):
            let decoded = try await gate.run { try await flash.transcribe(samples) }
            return TranscriptResult(
                text: decoded.text,
                language: "en",
                segments: timestamps ? Timeline.segments(from: decoded.words) : nil
            )
        }
    }

    private func model(for id: ModelID) async throws -> Loaded {
        if let model = loaded[id] { return model }
        if let pending = loading[id] { return try await pending.value }
        guard let hub = HubModel.forModel(id) else {
            throw EngineError("\(id.rawValue) is not a Parakeet model")
        }
        let directory = try store.readyDirectory(for: hub)
        let task = Task { try await Self.load(id, from: directory) }
        loading[id] = task
        do {
            let model = try await task.value
            // A delete during the load removes the pending task; the result is then dropped.
            if loading[id] == task {
                loading[id] = nil
                loaded[id] = model
            }
            return model
        } catch {
            if loading[id] == task {
                loading[id] = nil
            }
            throw error
        }
    }

    private static func load(_ id: ModelID, from directory: URL) async throws -> Loaded {
        Log.info("loading \(id.rawValue) from \(directory.path)")
        let started = ContinuousClock.now
        let model: Loaded
        switch id {
        case .parakeetTdtV3, .parakeetTdtV2:
            let version: AsrModelVersion = id == .parakeetTdtV3 ? .v3 : .v2
            let models = try AsrModels.loadLocal(from: directory, version: version)
            let manager = AsrManager(config: .default)
            try await manager.loadModels(models)
            model = .tdt(manager, version, models)
        case .parakeetFlash:
            model = .flash(try await FlashTranscriber.load(from: directory))
        case .whisperLargeV3Turbo, .whisperHinglish, .appleSpeech, .appleIntelligence, .qwen3Instruct,
            .speakerDiarization, .madlad:
            throw EngineError("\(id.rawValue) is not a Parakeet model")
        }
        Log.info("loaded \(id.rawValue) in \(ContinuousClock.now - started)")
        return model
    }

    private struct TdtDecoded: Sendable {
        let text: String
        /// FluidAudio's confidence (absent for an empty clip, and when not a finite number, which
        /// JSON cannot carry).
        let confidence: Float?
        let words: [TimedWord]
    }

    private static func transcribeTdt(
        manager: AsrManager,
        version: AsrModelVersion,
        samples: [Float],
        code: String?
    ) async throws -> TdtDecoded {
        guard !samples.isEmpty else { return TdtDecoded(text: "", confidence: nil, words: []) }
        // The model rejects clips under its minimum length; a short word padded with silence
        // still transcribes.
        let minimum = ASRConstants.minimumRequiredSamples(forSampleRate: ASRConstants.sampleRate)
        let audio = samples.count < minimum ? samples + [Float](repeating: 0, count: minimum - samples.count) : samples
        var state = TdtDecoderState.make(decoderLayers: version.decoderLayers)
        // The language hint filters tokens by script, which only the v3 joint supports.
        let hint = version == .v3 ? code.flatMap { Language(rawValue: $0) } : nil
        let result = try await manager.transcribe(audio, decoderState: &state, language: hint)
        let confidence = result.confidence.isFinite ? result.confidence : nil
        let words = buildWordTimings(from: result.tokenTimings ?? []).map {
            TimedWord(text: $0.word, start: $0.startTime, end: $0.endTime)
        }
        return TdtDecoded(
            text: result.text.trimmingCharacters(in: .whitespacesAndNewlines),
            confidence: confidence,
            words: words
        )
    }
}

/// Runs async operations strictly one after another, in submission order. Cancelling a caller
/// cancels its operation, and an operation whose caller is gone before its turn never starts.
actor SerialGate {
    private var tail: Task<Void, Never>?

    func run<T: Sendable>(_ operation: @escaping @Sendable () async throws -> T) async throws -> T {
        let previous = tail
        let task = Task<T, any Error> {
            await previous?.value
            try Task.checkCancellation()
            return try await operation()
        }
        tail = Task { _ = try? await task.value }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
    }
}
