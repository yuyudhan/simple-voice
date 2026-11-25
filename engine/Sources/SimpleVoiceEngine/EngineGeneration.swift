// FilePath: engine/Sources/SimpleVoiceEngine/EngineGeneration.swift
// The engine's text-generation requests (`polish`, `generate`, `prepare_generate`): Apple
// Intelligence or the local language model.

import Foundation

extension Engine {
    func polish(_ params: PolishParams, progress: ProgressSink) async throws -> PolishResult {
        let id = try ModelID.parse(params.model ?? ModelID.appleIntelligence.rawValue)
        if id == .appleIntelligence {
            guard #available(macOS 26, *) else {
                throw EngineError("Apple Intelligence requires macOS 26 or later")
            }
            return try await AppleIntelligence.polish(params)
        }
        // A rewrite is about as long as its input: roughly a token per three bytes, plus room.
        let budget = min(2_048, params.user.utf8.count / 3 + 256)
        return try await generate(
            id, system: params.system, shots: params.shots, user: params.user, maxTokens: budget, temperature: 0,
            progress: progress)
    }

    func generate(_ params: GenerateParams, progress: ProgressSink) async throws -> PolishResult {
        if let maxTokens = params.maxTokens, maxTokens < 1 {
            throw EngineError("generate: `maxTokens` must be at least 1")
        }
        return try await generate(
            try ModelID.parse(params.model),
            system: params.system,
            shots: params.shots ?? [],
            user: params.user,
            maxTokens: params.maxTokens ?? 1_024,
            temperature: params.temperature ?? 0.2,
            progress: progress
        )
    }

    private func generate(
        _ id: ModelID,
        system: String,
        shots: [PolishShot],
        user: String,
        maxTokens: Int,
        temperature: Double,
        progress: ProgressSink
    ) async throws -> PolishResult {
        switch id {
        case .appleIntelligence:
            guard #available(macOS 26, *) else {
                throw EngineError("Apple Intelligence requires macOS 26 or later")
            }
            return try await AppleIntelligence.generate(
                system: system, shots: shots, user: user, maxTokens: maxTokens, temperature: temperature)
        case .qwen3Instruct:
            return try await languageModel.generate(
                ChatTurn.conversation(system: system, shots: shots, user: user), maxTokens: maxTokens,
                temperature: temperature, progress: progress)
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash, .fluidSpeechPico, .whisperLargeV3Turbo, .whisperHinglish,
            .appleSpeech, .speakerDiarization, .madlad, .parakeetVocabulary:
            throw EngineError("\(id.rawValue) is not a text model")
        }
    }

    /// Readies a text model for a `generate` that starts with these instructions and examples.
    func prepareGenerate(_ params: PrepareParams) async throws {
        let shots = params.shots ?? []
        switch try ModelID.parse(params.model) {
        case .appleIntelligence:
            guard #available(macOS 26, *) else { throw EngineError("Apple Intelligence requires macOS 26 or later") }
            try await AppleIntelligence.prepare(system: params.system, shots: shots)
        case .qwen3Instruct:
            try await languageModel.prepare(ChatTurn.conversation(system: params.system, shots: shots, user: nil))
        default:
            throw EngineError("\(params.model) is not a text model")
        }
    }
}
