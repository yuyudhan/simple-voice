// FilePath: engine/Sources/SimpleVoiceEngine/Indic.swift
// Indic (Hinglish): Bodhan AI's Indic-Transcribe-Flex (Canary-1B-v2 fine-tune) in FluidAudio's
// Canary Core ML layout, decoding Hindi with the romanized prompt so it writes Roman Hinglish
// itself, with no transliteration step. Kept resident like the other local models, on the GPU
// (see IndicPack.computeUnits), and warmed up while it loads.

import CoreML
import EngineCore
import FluidAudio
import Foundation

actor IndicEngine {
    private let store: ModelStore
    private let gate = SerialGate()
    private var loaded: CanaryManager?
    private var loading: Task<CanaryManager, any Error>?
    /// The files FluidAudio's int4 Canary loader reads, plus the licence and notice the Indic
    /// Open Model License requires to travel with the weights.
    static let requiredEntries =
        ModelNames.Canary.requiredModels(precision: .int4).sorted() + ["Indic_Open_Model_License.md", "NOTICE.md"]

    init(store: ModelStore) {
        self.store = store
    }

    func preload() async throws {
        _ = try await manager()
    }

    func unload() {
        loading?.cancel()
        loading = nil
        guard loaded != nil else { return }
        loaded = nil
        Log.info("unloaded \(ModelID.indicHinglish.rawValue)")
    }

    /// Romanised Hinglish, like Hinglish Whisper: no language is reported for it.
    func transcribe(samples: [Float]) async throws -> TranscriptResult {
        guard !samples.isEmpty else { return TranscriptResult(text: "", language: nil) }
        let manager = try await manager()
        let text = try await gate.run { try await manager.transcribe(audio: samples) }
        return TranscriptResult(text: text.trimmingCharacters(in: .whitespacesAndNewlines), language: nil)
    }

    private func manager() async throws -> CanaryManager {
        if let loaded { return loaded }
        if let loading { return try await loading.value }
        guard let hub = HubModel.forModel(.indicHinglish) else {
            throw EngineError("\(ModelID.indicHinglish.rawValue) has no download source")
        }
        let directory = try store.readyDirectory(for: hub)
        let task = Task { try await Self.load(from: directory) }
        loading = task
        do {
            let manager = try await task.value
            // A delete during the load removes the pending task; the result is then dropped.
            if loading == task {
                loading = nil
                loaded = manager
            }
            return manager
        } catch {
            if loading == task {
                loading = nil
            }
            throw error
        }
    }

    private static func load(from directory: URL) async throws -> CanaryManager {
        let id = ModelID.indicHinglish.rawValue
        Log.info("loading \(id) from \(directory.path)")
        let started = ContinuousClock.now
        // FluidAudio force-casts vocab.json, so a malformed pack is turned away here instead.
        let vocabulary = directory.appendingPathComponent(ModelNames.Canary.vocabularyFile)
        try IndicPack.validateVocabulary(try Data(contentsOf: vocabulary))
        // Loaded here rather than through CanaryModels.load, which pins the parts to the Neural
        // Engine. The int4 parts return float16, which FluidAudio 0.17.4's Canary decode loop
        // cannot read without trapping (see Float32Outputs), so they are handed on as float32.
        let models = CanaryModels(
            preprocessor: try part(.preprocessor, ModelNames.Canary.preprocessor, in: directory),
            encoder: Float32Outputs(try part(.encoder, ModelNames.Canary.encoderInt4, in: directory)),
            decoder: Float32Outputs(try part(.decoder, ModelNames.Canary.decoderInt4, in: directory)),
            projection: Float32Outputs(try part(.projection, ModelNames.Canary.projection, in: directory)),
            tokenizer: try Tokenizer(vocabPath: vocabulary))
        let manager = CanaryManager(models: models, prompt: IndicPack.hinglishPrompt)
        // Core ML prepares each part on its first prediction (about a second on the GPU), so one
        // pass over a second of silence runs here and the first dictation does not pay for it.
        _ = try await manager.transcribe(audio: [Float](repeating: 0, count: CanaryConfig.sampleRate))
        Log.info("loaded \(id) in \(ContinuousClock.now - started)")
        return manager
    }

    private static func part(_ part: IndicPack.Part, _ name: String, in directory: URL) throws -> MLModel {
        let configuration = MLModelConfiguration()
        configuration.computeUnits = IndicPack.computeUnits(for: part)
        let url = directory.appendingPathComponent("\(name).mlmodelc")
        return try MLModel(contentsOf: url, configuration: configuration)
    }
}
