// FilePath: engine/Tests/EngineCoreChecks/IndicPackChecks.swift
// The Indic (Hinglish) model is Indic-Transcribe-Flex in FluidAudio's Canary layout. FluidAudio
// reads its vocab.json as an id -> piece object (a list crashes it) and decodes with a prompt of
// Flex's own token ids, so a pack is checked for both before it is loaded.

import CoreML
import EngineCore
import Foundation

@MainActor func checkIndicPack() {
    check(
        IndicPack.hinglishPrompt == [7, 4, 18, 89, 89, 5, 9, 10, 13, 15],
        "Hinglish decodes Hindi with <|romanized|> (10), not <|noromanized|> (11)")

    let pieces = flexPieces()
    do {
        try IndicPack.validateVocabulary(try vocabularyObject(pieces))
    } catch {
        check(false, "a Flex vocabulary is accepted: \(error)")
    }

    expectRejection(
        try JSONSerialization.data(withJSONObject: pieces), mentioning: "object",
        "a vocabulary written as a JSON list (FluidAudio force-casts to an object)")

    var noRomanized = pieces
    noRomanized[10] = "<|noromanized|>"
    expectRejection(
        try vocabularyObject(noRomanized), mentioning: "<|romanized|>",
        "a tokenizer whose id 10 is not <|romanized|> (Canary-1B-v2's layout)")

    expectRejection(
        try vocabularyObject(Array(pieces.prefix(89))), mentioning: "<|hi|>",
        "a vocabulary missing the prompt's language token")

    // Measured on an M5 (T3026): with the Neural Engine allowed, Core ML runs every op of the
    // int4 encoder on the CPU (174 ms a pass) and the decoder takes 24 ms a step; on the GPU they
    // take 21 ms and 12 ms, and the parts load in about a second instead of a minute.
    check(IndicPack.computeUnits(for: .encoder) == .cpuAndGPU, "the Indic encoder runs on the GPU")
    check(IndicPack.computeUnits(for: .decoder) == .cpuAndGPU, "the Indic decoder runs on the GPU")
    check(
        IndicPack.computeUnits(for: .preprocessor) == .cpuOnly,
        "the mel preprocessor stays on the CPU in float32 (its power spectrum overflows float16)")
    check(IndicPack.computeUnits(for: .projection) == .cpuAndNeuralEngine, "the projection stays on the Neural Engine")
}

/// Flex's first special tokens at their real ids, padded with plain pieces up to `<|hi|>` (89).
private func flexPieces() -> [String] {
    var pieces = (0..<IndicPack.minimumPieces).map { "▁p\($0)" }
    let special: [Int: String] = [
        2: "<pad>", 3: "<|endoftext|>", 4: "<|startoftranscript|>", 5: "<|pnc|>", 7: "<|startofcontext|>",
        9: "<|noitn|>", 10: "<|romanized|>", 11: "<|noromanized|>", 13: "<|notimestamp|>", 15: "<|nodiarize|>",
        18: "<|emo:undefined|>", 89: "<|hi|>",
    ]
    for (id, piece) in special { pieces[id] = piece }
    return pieces
}

private func vocabularyObject(_ pieces: [String]) throws -> Data {
    let object = Dictionary(uniqueKeysWithValues: pieces.enumerated().map { (String($0.offset), $0.element) })
    return try JSONSerialization.data(withJSONObject: object)
}

@MainActor private func expectRejection(
    _ data: @autoclosure () throws -> Data, mentioning needle: String, _ what: String
) {
    do {
        try IndicPack.validateVocabulary(try data())
        check(false, "\(what) must be rejected")
    } catch {
        check("\(error)".contains(needle), "\(what): the error names \(needle): \(error)")
    }
}
