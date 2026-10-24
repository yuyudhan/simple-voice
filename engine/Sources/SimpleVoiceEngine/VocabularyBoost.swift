// FilePath: engine/Sources/SimpleVoiceEngine/VocabularyBoost.swift
// Personal dictionary boosting for Parakeet TDT. Parakeet takes no prompt, so the dictionary
// reaches it after decoding: FluidAudio's CTC keyword spotter (Parakeet CTC 110M, a separate
// download) scores every term against the audio, and its rescorer swaps transcript words for a
// term only where the audio supports the term better than what was written.

import FluidAudio
import Foundation

/// The resident CTC spotter plus the rescorer for the last dictionary it saw. Building a rescorer
/// tokenizes every term, so it is kept while the term list stays the same.
actor VocabularyBooster {
    /// What `CtcModels.loadDirect` and `CtcTokenizer.load` read from the model directory.
    static let requiredEntries = ["MelSpectrogram.mlmodelc", "AudioEncoder.mlmodelc", "vocab.json", "tokenizer.json"]

    /// What a rescore changed, for the transcript and the log line.
    struct Boosted: Sendable {
        let text: String
        let replacements: Int
    }

    private struct Prepared: Sendable {
        let terms: [String]
        let context: CustomVocabularyContext
        let rescorer: VocabularyRescorer
        let sizeConfig: ContextBiasingConstants.VocabSizeConfig
    }

    private let directory: URL
    private let tokenizer: CtcTokenizer
    private let spotter: CtcKeywordSpotter
    private var prepared: Prepared?

    private init(directory: URL, tokenizer: CtcTokenizer, spotter: CtcKeywordSpotter) {
        self.directory = directory
        self.tokenizer = tokenizer
        self.spotter = spotter
    }

    static func load(from directory: URL) async throws -> VocabularyBooster {
        let started = ContinuousClock.now
        let models = try await CtcModels.loadDirect(from: directory, variant: .ctc110m)
        let tokenizer = try await CtcTokenizer.load(from: directory)
        // The blank token sits right after the model's vocabulary.
        let spotter = CtcKeywordSpotter(models: models, blankId: models.vocabulary.count)
        Log.info("loaded \(ModelID.parakeetVocabulary.rawValue) in \(ContinuousClock.now - started)")
        return VocabularyBooster(directory: directory, tokenizer: tokenizer, spotter: spotter)
    }

    /// The transcript with dictionary terms the audio supports written in, or nil when nothing
    /// changed. Failures are logged and absorbed: boosting never costs the user a dictation.
    func rescore(
        text: String,
        tokenTimings: [TokenTiming],
        samples: [Float],
        terms: [String]
    ) async -> Boosted? {
        guard !text.isEmpty, !tokenTimings.isEmpty, !samples.isEmpty else { return nil }
        do {
            let prepared = try await self.prepared(for: terms)
            guard !prepared.context.terms.isEmpty else { return nil }
            let spotted = try await spotter.spotKeywordsWithLogProbs(
                audioSamples: samples,
                customVocabulary: prepared.context,
                minScore: nil
            )
            guard !spotted.logProbs.isEmpty else { return nil }
            let output = prepared.rescorer.ctcTokenRescore(
                transcript: text,
                tokenTimings: tokenTimings,
                logProbs: spotted.logProbs,
                frameDuration: spotted.frameDuration,
                cbw: prepared.sizeConfig.cbw,
                marginSeconds: 0.5,
                minSimilarity: max(prepared.sizeConfig.minSimilarity, prepared.context.minSimilarity)
            )
            guard output.wasModified else { return nil }
            // The rescorer rebuilds its text from word timings, which loses the punctuation and
            // casing TDT wrote; its decisions are applied to the transcript itself instead. Its
            // replacements are not in transcript order, so each is found from the start.
            var rewritten = text
            var applied = 0
            for replacement in output.replacements where replacement.shouldReplace {
                guard let word = replacement.replacementWord,
                    Self.keepsEveryWord(replacement.originalWord, replacedBy: word),
                    let range = Self.wholeWords(replacement.originalWord, in: rewritten)
                else { continue }
                rewritten.replaceSubrange(range, with: word)
                applied += 1
            }
            return applied == 0 ? nil : Boosted(text: rewritten, replacements: applied)
        } catch {
            Log.info("dictionary boosting skipped: \(error.localizedDescription)")
            return nil
        }
    }

    /// A word TDT ran together from two names ("Ujvalhimanshu") must not collapse into one of
    /// them: a replacement half again shorter than what it replaces would drop a spoken word.
    static func keepsEveryWord(_ original: String, replacedBy replacement: String) -> Bool {
        let letters = { (text: String) in text.filter { $0.isLetter || $0.isNumber }.count }
        return letters(original) * 2 <= letters(replacement) * 3
    }

    /// The first occurrence of `words` that begins and ends on a word frontier, matched
    /// case-insensitively (the rescorer reports words lowercased at times). The rescorer's words
    /// can carry the punctuation TDT attached; that stays in the text.
    static func wholeWords(_ words: String, in text: String) -> Range<String.Index>? {
        let words = words.trimmingCharacters(in: .punctuationCharacters.union(.whitespaces))
        guard !words.isEmpty else { return nil }
        var from = text.startIndex
        while let range = text.range(of: words, options: [.caseInsensitive], range: from..<text.endIndex) {
            let before = range.lowerBound == text.startIndex ? nil : text[text.index(before: range.lowerBound)]
            let after = range.upperBound == text.endIndex ? nil : text[range.upperBound]
            let isLetter = { (c: Character?) in c.map { $0.isLetter || $0.isNumber } ?? false }
            if !isLetter(before) && !isLetter(after) {
                return range
            }
            from = text.index(after: range.lowerBound)
        }
        return nil
    }

    private func prepared(for terms: [String]) async throws -> Prepared {
        if let prepared, prepared.terms == terms { return prepared }
        let vocabularyTerms = terms.compactMap { text -> CustomVocabularyTerm? in
            let ids = tokenizer.encode(text)
            return ids.isEmpty ? nil : CustomVocabularyTerm(text: text, ctcTokenIds: ids)
        }
        let context = CustomVocabularyContext(terms: vocabularyTerms)
        let rescorer = try await VocabularyRescorer.create(
            spotter: spotter,
            vocabulary: context,
            config: .default,
            ctcModelDirectory: directory
        )
        let made = Prepared(
            terms: terms,
            context: context,
            rescorer: rescorer,
            sizeConfig: ContextBiasingConstants.rescorerConfig(forVocabSize: vocabularyTerms.count)
        )
        prepared = made
        return made
    }
}
