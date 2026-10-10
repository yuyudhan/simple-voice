// FilePath: engine/Sources/SimpleVoiceEngine/Whisper.swift
// Whisper Turbo and Hinglish Whisper batch transcription through WhisperKit, with models kept resident.

import CoreML
import Foundation
import WhisperKit

/// What one Whisper transcription is asked to do.
struct WhisperRequest: Sendable {
    /// Pins recognition to one language; skips detection.
    let language: String?
    /// Languages detection may choose from; empty means any Whisper language.
    let languages: [String]
    /// Conditioning prompt, typically the personal dictionary.
    let prompt: String?
}

/// Loaded Whisper models, one per model id, kept in memory so a dictation only pays for inference.
/// Every call into WhisperKit (transcription and unloading) runs through one gate, one at a time.
actor WhisperEngine {
    private let store: ModelStore
    private let gate = SerialGate()
    private var loaded: [ModelID: ResidentWhisper] = [:]
    private var loading: [ModelID: Task<ResidentWhisper, any Error>] = [:]

    init(store: ModelStore) {
        self.store = store
    }

    func preload(_ id: ModelID) async throws {
        _ = try await model(for: id)
    }

    func unload(_ id: ModelID) async {
        loading.removeValue(forKey: id)?.cancel()
        guard let model = loaded.removeValue(forKey: id) else { return }
        do {
            try await gate.run { await model.unload() }
        } catch {
            Log.error("unloading \(id.rawValue) failed: \(describe(error))")
        }
        Log.info("unloaded \(id.rawValue)")
    }

    func transcribe(_ id: ModelID, samples: [Float], request: WhisperRequest) async throws -> TranscriptResult {
        let model = try await model(for: id)
        return try await gate.run { try await model.transcribe(samples, request: request) }
    }

    private func model(for id: ModelID) async throws -> ResidentWhisper {
        if let model = loaded[id] { return model }
        if let pending = loading[id] { return try await pending.value }
        guard let hub = HubModel.forModel(id) else {
            throw EngineError("\(id.rawValue) is not a Whisper model")
        }
        let directory = try store.readyDirectory(for: hub)
        let task = Task { try await ResidentWhisper.load(id, from: directory) }
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
}

/// One loaded WhisperKit pipeline. WhisperKit is a non-Sendable class; `WhisperEngine` only calls
/// these methods inside its SerialGate, so the pipeline is never used by two tasks at once.
private final class ResidentWhisper: @unchecked Sendable {
    private let id: ModelID
    private let kit: WhisperKit

    private init(id: ModelID, kit: WhisperKit) {
        self.id = id
        self.kit = kit
    }

    /// Loads from the model directory only: downloads are off and the tokenizer files sit at the
    /// top of the directory, where WhisperKit finds them before it would try the network.
    static func load(_ id: ModelID, from directory: URL) async throws -> ResidentWhisper {
        Log.info("loading \(id.rawValue) from \(directory.path)")
        let started = ContinuousClock.now
        // No prewarm: it compiles each model and then loads it again, doubling the first load
        // for a peak-memory saving the helper does not need.
        let config = WhisperKitConfig(
            modelFolder: directory.path,
            tokenizerFolder: directory,
            verbose: false,
            logLevel: .none,
            prewarm: false,
            load: true,
            download: false
        )
        let kit = try await WhisperKit(config)
        guard kit.modelState == .loaded, kit.tokenizer != nil else {
            throw EngineError("\(id.rawValue) did not load; delete it and download it again")
        }
        Log.info("loaded \(id.rawValue) in \(ContinuousClock.now - started)")
        return ResidentWhisper(id: id, kit: kit)
    }

    func unload() async {
        await kit.unloadModels()
    }

    func transcribe(_ samples: [Float], request: WhisperRequest) async throws -> TranscriptResult {
        // Hinglish Whisper was fine-tuned to write romanised Hinglish behind the `en` token and
        // without timestamps, which is how its model card decodes it; with timestamps it drops the
        // first word. Its `en` does not mean English, so no language is reported for it.
        let romanised = id == .whisperHinglish
        let pinned = romanised ? "en" : LanguageCode.primary(request.language)
        let reported = romanised ? nil : pinned
        guard !samples.isEmpty else { return TranscriptResult(text: "", language: reported) }
        guard let tokenizer = kit.tokenizer else {
            throw EngineError("\(id.rawValue) has no tokenizer; delete it and download it again")
        }
        let language: String
        if let pinned {
            language = pinned
        } else {
            language = try await detectLanguage(samples, among: request.languages, tokenizer: tokenizer)
        }

        var options = DecodingOptions(
            language: language,
            usePrefillPrompt: true,
            detectLanguage: false,
            withoutTimestamps: romanised
        )
        let prompt = request.prompt?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if !prompt.isEmpty {
            // Encoded the way WhisperKit's own CLI does it: a leading space, special tokens dropped.
            options.promptTokens = tokenizer.encode(text: " " + prompt)
                .filter { $0 < tokenizer.specialTokens.specialTokenBegin }
        }
        // Hinglish Whisper misses a word spoken in the first moments of the audio, and dictation
        // clips start right as the key goes down; half a second of leading silence keeps it.
        let leadIn = romanised ? [Float](repeating: 0, count: WhisperKit.sampleRate / 2) : []
        // WhisperKit stops decoding `windowClipTime` (1 s) before the end of the audio to avoid
        // hallucinations, so it returns nothing for a clip that short. Silence padding up to one
        // second past that point lets a single short word through.
        let minimum = Int((options.windowClipTime + 1) * Float(WhisperKit.sampleRate))
        let padded = leadIn + samples
        let audio = padded.count < minimum ? padded + [Float](repeating: 0, count: minimum - padded.count) : padded
        let results = try await kit.transcribe(audioArray: audio, decodeOptions: options)
        let result = results.count == 1 ? results[0] : TranscriptionUtilities.mergeTranscriptionResults(results)
        return TranscriptResult(
            text: result.text.trimmingCharacters(in: .whitespacesAndNewlines),
            language: romanised ? nil : language,
            quality: Self.quality(of: result.segments)
        )
    }

    /// WhisperKit's language detection (the first 30 s through the encoder and one decoder step),
    /// except that the language token is chosen among `codes` only. WhisperKit's own detection
    /// method reports the probability of the winning language alone, so it cannot rank the
    /// allowed ones.
    private func detectLanguage(
        _ samples: [Float],
        among codes: [String],
        tokenizer: any WhisperTokenizer
    ) async throws -> String {
        let allowed = Set(
            codes.compactMap { LanguageCode.primary($0) }.compactMap {
                tokenizer.convertTokenToId("<|\($0)|>")
            }
        ).intersection(tokenizer.allLanguageTokens)
        if allowed.count == 1, let token = allowed.first {
            return Self.languageCode(of: token, tokenizer: tokenizer)
        }
        guard kit.textDecoder.isModelMultilingual else {
            throw EngineError("\(id.rawValue) cannot detect the spoken language")
        }
        let window = kit.featureExtractor.windowSamples ?? Constants.defaultWindowSamples
        guard let audio = kit.audioProcessor.padOrTrim(fromArray: samples, startAt: 0, toLength: window),
            let mel = try await kit.featureExtractor.logMelSpectrogram(fromAudio: audio),
            let encoded = try await kit.audioEncoder.encodeFeatures(mel)
        else {
            throw EngineError("\(id.rawValue) could not encode the audio for language detection")
        }
        let inputs = try kit.textDecoder.prepareDecoderInputs(
            withPrompt: [tokenizer.specialTokens.startOfTranscriptToken])
        let sampler = LanguageSampler(candidates: allowed.isEmpty ? tokenizer.allLanguageTokens : allowed)
        let result = try await kit.textDecoder.detectLanguage(
            from: encoded,
            using: inputs,
            sampler: sampler,
            options: DecodingOptions(),
            temperature: 0
        )
        return result.language
    }

    private static func languageCode(of token: Int, tokenizer: any WhisperTokenizer) -> String {
        tokenizer.decode(tokens: [token]).trimmingCharacters(in: CharacterSet(charactersIn: "<|>"))
    }

    /// Token-weighted means of log probability and no-speech probability (a segment without
    /// tokens weighs 1) and the largest compression ratio. Values that are not finite, such as
    /// the infinite compression ratio WhisperKit reports for an empty segment, are left out.
    static func quality(of segments: [TranscriptionSegment]) -> TranscriptQuality? {
        var logprob = WeightedMean()
        var noSpeech = WeightedMean()
        var compression: Float?
        for segment in segments {
            let weight = Float(max(segment.tokens.count, 1))
            logprob.add(segment.avgLogprob, weight: weight)
            noSpeech.add(segment.noSpeechProb, weight: weight)
            if segment.compressionRatio.isFinite {
                compression = max(compression ?? segment.compressionRatio, segment.compressionRatio)
            }
        }
        let quality = TranscriptQuality(
            avgLogprob: logprob.value,
            compressionRatio: compression,
            noSpeechProb: noSpeech.value
        )
        let empty = quality.avgLogprob == nil && quality.compressionRatio == nil && quality.noSpeechProb == nil
        return empty ? nil : quality
    }
}

private struct WeightedMean {
    private var total: Float = 0
    private var weights: Float = 0

    mutating func add(_ value: Float, weight: Float) {
        guard value.isFinite else { return }
        total += value * weight
        weights += weight
    }

    var value: Float? { weights > 0 ? total / weights : nil }
}

/// Picks the most likely language token among `candidates` from the logits of the first decoder
/// step (WhisperKit has already masked every non-language token).
private struct LanguageSampler: TokenSampling {
    /// Sorted, so equal logits always resolve to the same language.
    private let candidates: [Int]

    init(candidates: Set<Int>) {
        self.candidates = candidates.sorted()
    }

    func update(tokens: [Int], logits: MLMultiArray, logProbs: [Float]) async -> SamplingResult {
        let scored = candidates.filter { $0 < logits.count }.map { ($0, logits[$0].floatValue) }
        guard let best = scored.max(by: { $0.1 < $1.1 }) else {
            return SamplingResult(tokens: tokens, logProbs: logProbs, completed: true)
        }
        // Log-softmax over the candidates, so the reported probability is relative to them.
        let normalizer = best.1 + log(scored.reduce(Float(0)) { $0 + exp($1.1 - best.1) })
        return SamplingResult(tokens: tokens + [best.0], logProbs: logProbs + [best.1 - normalizer], completed: true)
    }

    func finalize(tokens: [Int], logProbs: [Float]) -> SamplingResult {
        SamplingResult(tokens: tokens, logProbs: logProbs, completed: true)
    }
}
