// FilePath: engine/Sources/SimpleVoiceEngine/LocalLanguageModel.swift
// Local text generation for Macs without Apple Intelligence (and for languages it declines):
// Qwen3 4B Instruct 2507 (Apache-2.0, Q4_K_M GGUF) on llama.cpp with Metal. The weights are a
// user-initiated download; prompts and outputs never leave the process.

import EngineCore
import Foundation
import llama

/// One chat turn handed to the model's chat template.
struct ChatTurn: Sendable {
    let role: String
    let content: String
}

/// The downloaded Qwen model, kept resident between requests (see `ResidentModel`).
struct LocalLanguageModel: Sendable {
    static let qwenFile = "Qwen3-4B-Instruct-2507-Q4_K_M.gguf"
    /// Prompt plus answer. The KV cache for 8 k tokens is about 1.2 GB on top of 2.5 GB of weights.
    static let contextTokens = 8_192
    /// Reloading takes about 2.7 s, longer than a whole warm formatting pass, so the model stays
    /// in memory through an hour without dictations instead of the translator's ten minutes.
    static let idleUnload: Duration = .seconds(3_600)

    private let resident: ResidentModel<LlamaRuntime>

    init(store: ModelStore) {
        resident = ResidentModel(id: .qwen3Instruct, file: Self.qwenFile, store: store, idleUnload: Self.idleUnload) {
            path in
            try LlamaRuntime.load(path: path, contextTokens: Self.contextTokens)
        }
    }

    func preload() async throws {
        try await resident.preload()
    }

    func unload() async {
        await resident.unload()
    }

    func generate(
        _ turns: [ChatTurn], maxTokens: Int, temperature: Double, progress: ProgressSink?
    )
        async throws -> PolishResult
    {
        try await resident.run { runtime in
            try await runtime.generate(turns, maxTokens: maxTokens, temperature: temperature, progress: progress)
        }
    }

    /// Loads the model and reads `turns` into its cache, so a request that starts with them
    /// decodes only what follows.
    func prepare(_ turns: [ChatTurn]) async throws {
        try await resident.run { runtime in try await runtime.prefill(turns) }
    }
}

extension ChatTurn {
    /// The system prompt, the examples as user and assistant turns, then `user` when given.
    static func conversation(system: String, shots: [PolishShot], user: String?) -> [ChatTurn] {
        var turns = [ChatTurn(role: "system", content: system)]
        for shot in shots {
            turns.append(ChatTurn(role: "user", content: shot.user))
            turns.append(ChatTurn(role: "assistant", content: shot.assistant))
        }
        if let user {
            turns.append(ChatTurn(role: "user", content: user))
        }
        return turns
    }
}

/// A loaded llama.cpp model and context. Only `LocalLanguageModel` uses it, one call at a time.
///
/// The context keeps the tokens of the previous request. Every formatting request starts with
/// the same instructions and examples (about 1,000 tokens), so a new request decodes only what
/// follows the prefix it shares with the cached tokens: the dictation's own turn.
final class LlamaRuntime: @unchecked Sendable {
    private static let batchSize = 512

    private let model: OpaquePointer
    private let context: OpaquePointer
    private let vocab: OpaquePointer
    private let contextTokens: Int
    /// The tokens whose keys and values sit in the context, at positions 0..<count of sequence 0.
    private var cache = PromptCache()

    private init(model: OpaquePointer, context: OpaquePointer, vocab: OpaquePointer, contextTokens: Int) {
        self.model = model
        self.context = context
        self.vocab = vocab
        self.contextTokens = contextTokens
    }

    deinit {
        llama_free(context)
        llama_model_free(model)
    }

    static func load(path: String, contextTokens: Int) throws -> LlamaRuntime {
        LlamaBackend.ensure()
        Log.info("loading \(ModelID.qwen3Instruct.rawValue)")
        let started = ContinuousClock.now
        var modelParams = llama_model_default_params()
        // Every layer on the GPU (Metal); Apple Silicon shares the memory anyway.
        modelParams.n_gpu_layers = 999
        guard let model = llama_model_load_from_file(path, modelParams) else {
            throw EngineError("qwen3-4b-instruct could not be loaded; delete it and download it again")
        }
        var contextParams = llama_context_default_params()
        let threads = Int32(max(1, min(8, ProcessInfo.processInfo.activeProcessorCount - 2)))
        contextParams.n_ctx = UInt32(contextTokens)
        contextParams.n_batch = UInt32(batchSize)
        contextParams.n_ubatch = UInt32(batchSize)
        contextParams.n_threads = threads
        contextParams.n_threads_batch = threads
        contextParams.no_perf = true
        guard let context = llama_init_from_model(model, contextParams) else {
            llama_model_free(model)
            throw EngineError("not enough memory for qwen3-4b-instruct's context")
        }
        guard let vocab = llama_model_get_vocab(model) else {
            llama_free(context)
            llama_model_free(model)
            throw EngineError("qwen3-4b-instruct has no vocabulary; delete it and download it again")
        }
        Log.info("loaded \(ModelID.qwen3Instruct.rawValue) in \(ContinuousClock.now - started)")
        return LlamaRuntime(model: model, context: context, vocab: vocab, contextTokens: Int(llama_n_ctx(context)))
    }

    func generate(
        _ turns: [ChatTurn], maxTokens: Int, temperature: Double, progress: ProgressSink?
    )
        async throws -> PolishResult
    {
        let budget = max(1, maxTokens)
        let tokens = try tokenize(try format(turns, openAnswer: true))
        guard tokens.count + budget <= contextTokens else {
            throw EngineError(
                "The text is too long for the local model (\(tokens.count) prompt tokens + \(budget) answer tokens; "
                    + "the limit is \(contextTokens))")
        }
        try await read(tokens)

        guard let sampler = Self.sampler(temperature: temperature) else {
            throw EngineError("cannot create the local model's sampler")
        }
        defer { llama_sampler_free(sampler) }
        var bytes: [UInt8] = []
        var piece = [CChar](repeating: 0, count: 256)
        var produced = 0
        var finished = false
        while produced < budget {
            try Task.checkCancellation()
            var token = llama_sampler_sample(sampler, context, -1)
            if llama_vocab_is_eog(vocab, token) {
                finished = true
                break
            }
            let length = llama_token_to_piece(vocab, token, &piece, Int32(piece.count), 0, false)
            if length > 0 {
                bytes.append(contentsOf: piece[0..<Int(length)].map { UInt8(bitPattern: $0) })
            }
            produced += 1
            let status = withUnsafeMutablePointer(to: &token) { pointer in
                llama_decode(context, llama_batch_get_one(pointer, 1))
            }
            guard status == 0 else {
                forgetCache()
                throw EngineError("the local model stopped generating (\(status))")
            }
            cache.append(token)
            if produced % 16 == 0 {
                await progress?.report(Double(produced) / Double(budget), "Generating")
                await Task.yield()
            }
        }
        let text = String(decoding: bytes, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        return PolishResult(text: text, finished: finished)
    }

    /// Reads `turns` (without an open answer) into the cache ahead of a request.
    func prefill(_ turns: [ChatTurn]) async throws {
        let tokens = try tokenize(try format(turns, openAnswer: false))
        guard tokens.count < contextTokens else { return }
        try await read(tokens)
    }

    /// Decodes the tokens the cache does not already hold ([`PromptCache.read`]).
    private func read(_ tokens: [llama_token]) async throws {
        let context = context
        let memory = llama_get_memory(context)
        try await cache.read(
            tokens,
            batchSize: Self.batchSize,
            trim: { position in llama_memory_seq_rm(memory, 0, Int32(position), -1) },
            clear: { llama_memory_clear(memory, true) },
            decode: { tokens in
                var batch = tokens
                let status = batch.withUnsafeMutableBufferPointer { buffer -> Int32 in
                    guard let base = buffer.baseAddress else { return -1 }
                    return llama_decode(context, llama_batch_get_one(base, Int32(buffer.count)))
                }
                guard status == 0 else {
                    throw EngineError("the local model failed to read the prompt (\(status))")
                }
            })
    }

    private func forgetCache() {
        let memory = llama_get_memory(context)
        cache.forget { llama_memory_clear(memory, true) }
    }

    /// Greedy at temperature 0 (deterministic polish); otherwise Qwen's recommended top-k 20,
    /// top-p 0.8 sampling.
    private static func sampler(temperature: Double) -> UnsafeMutablePointer<llama_sampler>? {
        guard let chain = llama_sampler_chain_init(llama_sampler_chain_default_params()) else { return nil }
        if temperature <= 0 {
            llama_sampler_chain_add(chain, llama_sampler_init_greedy())
        } else {
            llama_sampler_chain_add(chain, llama_sampler_init_top_k(20))
            llama_sampler_chain_add(chain, llama_sampler_init_top_p(0.8, 1))
            llama_sampler_chain_add(chain, llama_sampler_init_temp(Float(temperature)))
            llama_sampler_chain_add(chain, llama_sampler_init_dist(UInt32.max))
        }
        return chain
    }

    /// The model's own chat template (ChatML for Qwen), applied by llama.cpp; `openAnswer` adds
    /// the assistant turn's opening the model writes into.
    private func format(_ turns: [ChatTurn], openAnswer: Bool) throws -> String {
        let roles = turns.map { strdup($0.role) }
        let contents = turns.map { strdup($0.content) }
        defer {
            roles.forEach { free($0) }
            contents.forEach { free($0) }
        }
        var messages: [llama_chat_message] = []
        messages.reserveCapacity(turns.count)
        for (role, content) in zip(roles, contents) {
            guard let role, let content else { throw EngineError("out of memory formatting the prompt") }
            messages.append(llama_chat_message(role: UnsafePointer(role), content: UnsafePointer(content)))
        }
        let template = llama_model_chat_template(model, nil)
        var capacity = turns.reduce(256) { $0 + 2 * ($1.role.utf8.count + $1.content.utf8.count) }
        var buffer = [CChar](repeating: 0, count: capacity)
        var length = llama_chat_apply_template(
            template, &messages, messages.count, openAnswer, &buffer, Int32(capacity))
        if length > Int32(capacity) {
            capacity = Int(length) + 1
            buffer = [CChar](repeating: 0, count: capacity)
            length = llama_chat_apply_template(
                template, &messages, messages.count, openAnswer, &buffer, Int32(capacity))
        }
        guard length >= 0, length <= Int32(capacity) else {
            throw EngineError("the local model's chat template is not supported")
        }
        return String(decoding: buffer[0..<Int(length)].map { UInt8(bitPattern: $0) }, as: UTF8.self)
    }

    private func tokenize(_ text: String) throws -> [llama_token] {
        let byteCount = text.utf8.count
        var tokens = [llama_token](repeating: 0, count: byteCount + 8)
        let count = llama_tokenize(vocab, text, Int32(byteCount), &tokens, Int32(tokens.count), true, true)
        guard count >= 0 else { throw EngineError("the prompt could not be tokenized") }
        return Array(tokens.prefix(Int(count)))
    }
}
