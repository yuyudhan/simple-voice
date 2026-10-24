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
    /// The dictionary booster, loaded on the first dictation that sends a vocabulary once its
    /// model is downloaded.
    private var booster: VocabularyBooster?
    private var boosterLoading: Task<VocabularyBooster, any Error>?

    init(store: ModelStore) {
        self.store = store
    }

    func preload(_ id: ModelID) async throws {
        _ = try await model(for: id)
    }

    func unload(_ id: ModelID) async {
        if id == .parakeetVocabulary {
            boosterLoading?.cancel()
            boosterLoading = nil
            booster = nil
            Log.info("unloaded \(id.rawValue)")
            return
        }
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

    /// `vocabulary`: personal dictionary terms; TDT models rescore their transcript against them
    /// when the `parakeet-vocabulary` model is downloaded, and ignore them otherwise.
    func transcribe(
        _ id: ModelID,
        samples: [Float],
        language: String?,
        timestamps: Bool,
        vocabulary: [String] = []
    ) async throws -> TranscriptResult {
        let model = try await model(for: id)
        let code = LanguageCode.primary(language)
        switch model {
        case .tdt(let manager, let version, _):
            let booster = vocabulary.isEmpty ? nil : await vocabularyBooster()
            let decoded = try await gate.run {
                let decoded = try await Self.transcribeTdt(manager: manager, version: version, samples: samples, code: code)
                guard let booster else { return decoded }
                return await Self.boost(decoded, booster: booster, samples: samples, vocabulary: vocabulary)
            }
            // v2 is English only; v3 detects the language itself and only reports what was asked.
            return TranscriptResult(
                text: decoded.text,
                language: version == .v2 ? "en" : code,
                quality: decoded.confidence.map { TranscriptQuality(confidence: $0) },
                segments: timestamps ? Timeline.segments(from: decoded.words) : nil,
                dictionaryFixes: booster == nil ? nil : decoded.dictionaryFixes
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
            .speakerDiarization, .madlad, .parakeetVocabulary:
            throw EngineError("\(id.rawValue) is not a Parakeet model")
        }
        Log.info("loaded \(id.rawValue) in \(ContinuousClock.now - started)")
        return model
    }

    /// Loads the dictionary booster ahead of the first dictation; fails when it is not downloaded.
    func preloadVocabulary() async throws {
        guard let hub = HubModel.forModel(.parakeetVocabulary) else {
            throw EngineError("parakeet-vocabulary has no files")
        }
        _ = try store.readyDirectory(for: hub)
        guard await vocabularyBooster() != nil else {
            throw EngineError("parakeet-vocabulary could not be loaded; delete it and download it again")
        }
    }

    /// The resident booster, or nil when its model is not downloaded or fails to load (logged;
    /// the dictation goes on without it).
    private func vocabularyBooster() async -> VocabularyBooster? {
        if let booster { return booster }
        guard let hub = HubModel.forModel(.parakeetVocabulary),
            let directory = try? store.readyDirectory(for: hub)
        else { return nil }
        let task = boosterLoading ?? Task { try await VocabularyBooster.load(from: directory) }
        boosterLoading = task
        do {
            let loaded = try await task.value
            if boosterLoading == task {
                boosterLoading = nil
                booster = loaded
            }
            return loaded
        } catch {
            if boosterLoading == task {
                boosterLoading = nil
            }
            Log.info("dictionary boosting unavailable: \(error.localizedDescription)")
            return nil
        }
    }

    private struct TdtDecoded: Sendable {
        let text: String
        /// FluidAudio's confidence (absent for an empty clip, and when not a finite number, which
        /// JSON cannot carry).
        let confidence: Float?
        let words: [TimedWord]
        let tokenTimings: [TokenTiming]
        var dictionaryFixes = 0
    }

    private static func boost(
        _ decoded: TdtDecoded,
        booster: VocabularyBooster,
        samples: [Float],
        vocabulary: [String]
    ) async -> TdtDecoded {
        guard
            let boosted = await booster.rescore(
                text: decoded.text, tokenTimings: decoded.tokenTimings, samples: samples, terms: vocabulary)
        else { return decoded }
        var result = TdtDecoded(
            text: boosted.text, confidence: decoded.confidence, words: decoded.words,
            tokenTimings: decoded.tokenTimings)
        result.dictionaryFixes = boosted.replacements
        return result
    }

    private static func transcribeTdt(
        manager: AsrManager,
        version: AsrModelVersion,
        samples: [Float],
        code: String?
    ) async throws -> TdtDecoded {
        guard !samples.isEmpty else { return TdtDecoded(text: "", confidence: nil, words: [], tokenTimings: []) }
        // The model rejects clips under its minimum length; a short word padded with silence
        // still transcribes.
        let minimum = ASRConstants.minimumRequiredSamples(forSampleRate: ASRConstants.sampleRate)
        let audio = samples.count < minimum ? samples + [Float](repeating: 0, count: minimum - samples.count) : samples
        var state = TdtDecoderState.make(decoderLayers: version.decoderLayers)
        // The language hint filters tokens by script, which only the v3 joint supports.
        let hint = version == .v3 ? code.flatMap { Language(rawValue: $0) } : nil
        let result = try await manager.transcribe(audio, decoderState: &state, language: hint)
        let confidence = result.confidence.isFinite ? result.confidence : nil
        let timings = result.tokenTimings ?? []
        let words = buildWordTimings(from: timings).map {
            TimedWord(text: $0.word, start: $0.startTime, end: $0.endTime)
        }
        return TdtDecoded(
            text: result.text.trimmingCharacters(in: .whitespacesAndNewlines),
            confidence: confidence,
            words: words,
            tokenTimings: timings
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
