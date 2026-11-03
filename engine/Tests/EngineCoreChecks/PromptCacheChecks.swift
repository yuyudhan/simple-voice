// FilePath: engine/Tests/EngineCoreChecks/PromptCacheChecks.swift
// Prompt reuse counted in decoded tokens, not time: a second formatting request that starts with
// the same instructions decodes only its own turn. Without reuse Qwen's short warm request went
// from about 360 ms to 1,700 ms.

import EngineCore

/// A stand-in for the model context: records every batch it decodes, every trim and clear.
private final class FakeContext {
    var decoded: [[Int32]] = []
    var trims: [Int] = []
    var clears = 0
    var canTrim = true
    var failNextDecode = false

    var decodedTokens: [Int32] { decoded.flatMap { $0 } }

    func read(_ prompt: [Int32], into cache: inout PromptCache) async throws {
        try await cache.read(
            prompt,
            batchSize: 512,
            trim: { position in
                self.trims.append(position)
                return self.canTrim
            },
            clear: { self.clears += 1 },
            decode: { batch in
                if self.failNextDecode {
                    self.failNextDecode = false
                    throw DecodeFailed()
                }
                self.decoded.append(batch)
            })
    }
}

private struct DecodeFailed: Error {}

private let instructions: [Int32] = Array(1...1_000)

@MainActor func checkPromptCache() async {
    do {
        try await aSecondRequestWithTheSamePrefixDecodesOnlyItsOwnTurn()
        try await aPrefilledPrefixIsNotReadAgain()
        try await theSamePromptAgainDecodesItsLastTokenForTheLogits()
        try await generatedTokensCountAsCachedUntilTheNextPromptDiffers()
        try await aContextThatCannotTrimIsClearedAndReadWhole()
        try await aFailedDecodeClearsTheCache()
    } catch {
        check(false, "unexpected error: \(error)")
    }
}

@MainActor private func aSecondRequestWithTheSamePrefixDecodesOnlyItsOwnTurn() async throws {
    let context = FakeContext()
    var cache = PromptCache()
    try await context.read(instructions + [2_001, 2_002], into: &cache)
    check(context.decoded.map(\.count) == [512, 490], "first read batches: \(context.decoded.map(\.count))")

    context.decoded.removeAll()
    try await context.read(instructions + [3_001, 3_002, 3_003], into: &cache)
    check(
        context.decodedTokens == [3_001, 3_002, 3_003],
        "second read decoded \(context.decodedTokens.count) tokens, not only its own 3")
    check(context.trims == [1_000], "trims: \(context.trims)")
    check(cache.tokens == instructions + [3_001, 3_002, 3_003], "cache holds the second prompt")
}

@MainActor private func aPrefilledPrefixIsNotReadAgain() async throws {
    let context = FakeContext()
    var cache = PromptCache()
    try await context.read(instructions, into: &cache)
    context.decoded.removeAll()
    try await context.read(instructions + [4_001], into: &cache)
    check(context.decodedTokens == [4_001], "after a prefill decoded \(context.decodedTokens.count) tokens")
    check(context.trims.isEmpty, "a prefill is never trimmed: \(context.trims)")
}

@MainActor private func theSamePromptAgainDecodesItsLastTokenForTheLogits() async throws {
    let context = FakeContext()
    var cache = PromptCache()
    let prompt = instructions + [5_001]
    try await context.read(prompt, into: &cache)
    context.decoded.removeAll()
    try await context.read(prompt, into: &cache)
    check(context.decodedTokens == [5_001], "the same prompt decoded \(context.decodedTokens.count) tokens")
}

@MainActor private func generatedTokensCountAsCachedUntilTheNextPromptDiffers() async throws {
    let context = FakeContext()
    var cache = PromptCache()
    try await context.read(instructions + [6_001], into: &cache)
    cache.append(7_001)
    cache.append(7_002)
    context.decoded.removeAll()
    try await context.read(instructions + [6_002, 6_003], into: &cache)
    check(context.trims == [1_000], "the answer is trimmed after the shared prefix: \(context.trims)")
    check(context.decodedTokens == [6_002, 6_003], "decoded \(context.decodedTokens.count) tokens")
}

@MainActor private func aContextThatCannotTrimIsClearedAndReadWhole() async throws {
    let context = FakeContext()
    var cache = PromptCache()
    try await context.read(instructions + [8_001], into: &cache)
    context.decoded.removeAll()
    context.canTrim = false
    try await context.read(instructions + [8_002], into: &cache)
    check(context.clears == 1, "clears: \(context.clears)")
    check(context.decodedTokens.count == 1_001, "decoded \(context.decodedTokens.count) tokens")
}

@MainActor private func aFailedDecodeClearsTheCache() async throws {
    let context = FakeContext()
    var cache = PromptCache()
    context.failNextDecode = true
    do {
        try await context.read(instructions, into: &cache)
        check(false, "a failed decode must throw")
    } catch is DecodeFailed {
        check(context.clears == 1, "clears: \(context.clears)")
        check(cache.tokens.isEmpty, "the cache keeps \(cache.tokens.count) tokens after a failure")
    }
    try await context.read(instructions, into: &cache)
    check(context.decodedTokens.count == 1_000, "decoded \(context.decodedTokens.count) tokens")
}
