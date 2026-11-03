// FilePath: engine/Sources/EngineCore/PromptCache.swift
// The local language model's prompt reuse. Every formatting request starts with the same
// instructions and examples (about 1,000 tokens), so a request decodes only what follows the
// prefix it shares with the tokens already in the model's context: the dictation's own turn.

/// The tokens whose keys and values sit in a model context, at positions 0..<count of one
/// sequence. The context itself belongs to the caller, which trims, clears and decodes it through
/// the closures each call takes.
public struct PromptCache: Sendable {
    public private(set) var tokens: [Int32] = []

    public init() {}

    /// Reads `prompt` into the context: keeps the longest prefix the cache shares with it and
    /// decodes the rest in batches of at most `batchSize`. The last prompt token is always decoded
    /// again, since sampling needs its logits. `trim(position)` drops the context's tokens from
    /// `position` on and returns false when it cannot; then the context is cleared through
    /// `clear` and the whole prompt decoded. A failed `decode` clears the context too.
    public mutating func read(
        _ prompt: [Int32],
        batchSize: Int,
        trim: (Int) -> Bool,
        clear: () -> Void,
        decode: ([Int32]) async throws -> Void
    ) async throws {
        keepPrefix(of: prompt, trim: trim, clear: clear)
        var index = tokens.count
        while index < prompt.count {
            try Task.checkCancellation()
            let batch = Array(prompt[index..<min(index + max(1, batchSize), prompt.count)])
            do {
                try await decode(batch)
            } catch {
                forget(clear: clear)
                throw error
            }
            tokens.append(contentsOf: batch)
            index += batch.count
            await Task.yield()
        }
    }

    /// Records a token the caller decoded itself (a generated one).
    public mutating func append(_ token: Int32) {
        tokens.append(token)
    }

    /// Empties the cache and the context.
    public mutating func forget(clear: () -> Void) {
        clear()
        tokens.removeAll()
    }

    private mutating func keepPrefix(of prompt: [Int32], trim: (Int) -> Bool, clear: () -> Void) {
        var shared = 0
        let limit = min(tokens.count, prompt.count - 1)
        while shared < limit, tokens[shared] == prompt[shared] {
            shared += 1
        }
        guard shared < tokens.count else { return }
        if trim(shared) {
            tokens.removeSubrange(shared...)
        } else {
            forget(clear: clear)
        }
    }
}
