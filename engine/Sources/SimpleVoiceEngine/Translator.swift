// FilePath: engine/Sources/SimpleVoiceEngine/Translator.swift
// Meeting translation with a dedicated translation model: Google's MADLAD-400 3B MT (Apache-2.0,
// T5 encoder-decoder, 400+ languages) on llama.cpp. The target language is a `<2xx>` tag the
// model was trained with; the source language is detected by the model. Text never leaves the
// process.

import Foundation
import llama

struct TranslateParams: Decodable, Sendable {
    let model: String
    /// The language tag after `<2`, normally an ISO 639-1 code (`hi`, `es`).
    let targetLanguage: String
    let texts: [String]
}

struct TranslateResult: Encodable, Sendable {
    /// One translation per input text, in order.
    let texts: [String]
}

/// The downloaded MADLAD model, kept resident between requests (see `ResidentModel`).
struct LocalTranslator: Sendable {
    static let madladFile = "madlad400-3b-mt-q4_k_m.gguf"

    private let resident: ResidentModel<T5Runtime>

    init(store: ModelStore) {
        resident = ResidentModel(id: .madlad, file: Self.madladFile, store: store) { path in
            try T5Runtime.load(path: path)
        }
    }

    func preload() async throws {
        try await resident.preload()
    }

    func unload() async {
        await resident.unload()
    }

    func translate(_ texts: [String], into language: String, progress: ProgressSink) async throws -> [String] {
        try await resident.run { runtime in
            let tag = try runtime.languageTag(language)
            var translated: [String] = []
            translated.reserveCapacity(texts.count)
            for (index, text) in texts.enumerated() {
                try Task.checkCancellation()
                translated.append(try await runtime.translate(text, tag: tag))
                await progress.report(Double(index + 1) / Double(max(texts.count, 1)), "Translating")
            }
            return translated
        }
    }
}

/// A loaded T5 model and context. Only `LocalTranslator` uses it, one call at a time.
final class T5Runtime: @unchecked Sendable {
    /// Source tokens per encoder pass; longer texts are split at sentence or clause ends. MADLAD
    /// was trained on sentence-level pairs, and the encoder must fit one micro-batch.
    static let pieceTokens = 200
    private static let contextTokens = 1_024
    private static let batchTokens = 512

    private let model: OpaquePointer
    private let context: OpaquePointer
    private let vocab: OpaquePointer
    private let decoderStart: llama_token

    private init(model: OpaquePointer, context: OpaquePointer, vocab: OpaquePointer, decoderStart: llama_token) {
        self.model = model
        self.context = context
        self.vocab = vocab
        self.decoderStart = decoderStart
    }

    deinit {
        llama_free(context)
        llama_model_free(model)
    }

    static func load(path: String) throws -> T5Runtime {
        LlamaBackend.ensure()
        let name = ModelID.madlad.rawValue
        Log.info("loading \(name)")
        let started = ContinuousClock.now
        var modelParams = llama_model_default_params()
        // Every layer on the GPU (Metal); Apple Silicon shares the memory anyway.
        modelParams.n_gpu_layers = 999
        guard let model = llama_model_load_from_file(path, modelParams) else {
            throw EngineError("\(name) could not be loaded; delete it and download it again")
        }
        guard llama_model_has_encoder(model), let vocab = llama_model_get_vocab(model) else {
            llama_model_free(model)
            throw EngineError("\(name) is not an encoder-decoder translation model; delete it and download it again")
        }
        var contextParams = llama_context_default_params()
        let threads = Int32(max(1, min(8, ProcessInfo.processInfo.activeProcessorCount - 2)))
        contextParams.n_ctx = UInt32(contextTokens)
        contextParams.n_batch = UInt32(batchTokens)
        contextParams.n_ubatch = UInt32(batchTokens)
        contextParams.n_threads = threads
        contextParams.n_threads_batch = threads
        contextParams.no_perf = true
        guard let context = llama_init_from_model(model, contextParams) else {
            llama_model_free(model)
            throw EngineError("not enough memory for \(name)")
        }
        var start = llama_model_decoder_start_token(model)
        if start == LLAMA_TOKEN_NULL {
            start = llama_vocab_bos(vocab)
        }
        Log.info("loaded \(name) in \(ContinuousClock.now - started)")
        return T5Runtime(model: model, context: context, vocab: vocab, decoderStart: start)
    }

    /// The `<2xx>` token for a language. It must be one token of the model's vocabulary; any other
    /// string would be translated as text, not obeyed as an instruction. The tokenizer puts a
    /// word-boundary piece in front of a leading tag, which is not part of the tag.
    func languageTag(_ language: String) throws -> llama_token {
        let code = language.trimmingCharacters(in: .whitespacesAndNewlines)
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "_-"))
        guard !code.isEmpty, code.count <= 16, code.unicodeScalars.allSatisfy(allowed.contains) else {
            throw EngineError("invalid target language `\(language)`")
        }
        let tag = "<2\(code)>"
        let tokens = try tokenize(tag, special: false).filter {
            !piece($0).trimmingCharacters(in: .whitespaces).isEmpty
        }
        guard tokens.count == 1, piece(tokens[0]) == tag else {
            throw EngineError("\(ModelID.madlad.rawValue) does not support the language `\(code)`")
        }
        return tokens[0]
    }

    /// Translates `text`, piece by piece when it is longer than one encoder pass. Whitespace-only
    /// text stays empty.
    func translate(_ text: String, tag: llama_token) async throws -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }
        var parts: [String] = []
        for piece in try pieces(of: trimmed) {
            try Task.checkCancellation()
            let translated = try await translatePiece(piece, tag: tag)
            if !translated.isEmpty {
                parts.append(translated)
            }
        }
        return parts.joined(separator: " ")
    }

    /// The encoder input is "<2xx> text" tokenized the way llama.cpp's own tools do it (a
    /// word-boundary piece, the tag, the text) plus `</s>`. Feeding the tag token without that
    /// leading piece makes this GGUF repeat fragments instead of translating.
    private func translatePiece(_ text: String, tag: llama_token) async throws -> String {
        var input = try tokenize(piece(tag) + " " + text, special: false)
        let eos = llama_vocab_eos(vocab)
        if input.last != eos {
            input.append(eos)
        }
        guard input.count <= Self.batchTokens else {
            throw EngineError("a sentence is too long to translate (\(input.count) tokens)")
        }
        llama_memory_clear(llama_get_memory(context), true)
        let encoded = input.withUnsafeMutableBufferPointer { buffer -> Int32 in
            guard let base = buffer.baseAddress else { return -1 }
            return llama_encode(context, llama_batch_get_one(base, Int32(buffer.count)))
        }
        guard encoded == 0 else { throw EngineError("the translation model failed to read the text (\(encoded))") }

        guard let sampler = llama_sampler_chain_init(llama_sampler_chain_default_params()) else {
            throw EngineError("cannot create the translation model's sampler")
        }
        defer { llama_sampler_free(sampler) }
        llama_sampler_chain_add(sampler, llama_sampler_init_greedy())

        // A translation is rarely more than three times its source; the cap stops a degenerate
        // repetition loop from running to the end of the context.
        let budget = min(Self.contextTokens - 1, input.count * 3 + 32)
        var token = decoderStart
        var bytes: [UInt8] = []
        var piece = [CChar](repeating: 0, count: 256)
        for step in 0..<budget {
            try Task.checkCancellation()
            let status = withUnsafeMutablePointer(to: &token) { pointer in
                llama_decode(context, llama_batch_get_one(pointer, 1))
            }
            guard status == 0 else { throw EngineError("the translation model stopped (\(status))") }
            token = llama_sampler_sample(sampler, context, -1)
            if llama_vocab_is_eog(vocab, token) {
                break
            }
            let length = llama_token_to_piece(vocab, token, &piece, Int32(piece.count), 0, false)
            if length > 0 {
                bytes.append(contentsOf: piece[0..<Int(length)].map { UInt8(bitPattern: $0) })
            }
            if step % 32 == 31 {
                await Task.yield()
            }
        }
        input.removeAll()
        return String(decoding: bytes, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Splits text into pieces of at most `pieceTokens` tokens: whole sentences where they fit,
    /// otherwise clauses, otherwise runs of words.
    private func pieces(of text: String) throws -> [String] {
        if try tokenize(text, special: false).count <= Self.pieceTokens {
            return [text]
        }
        var pieces: [String] = []
        var current = ""
        for unit in try units(of: text) {
            let candidate = current.isEmpty ? unit : current + " " + unit
            if try tokenize(candidate, special: false).count <= Self.pieceTokens {
                current = candidate
            } else {
                if !current.isEmpty {
                    pieces.append(current)
                }
                current = unit
            }
        }
        if !current.isEmpty {
            pieces.append(current)
        }
        return pieces
    }

    /// Sentences; a sentence over the limit becomes clauses; a clause over it becomes word runs.
    private func units(of text: String) throws -> [String] {
        var result: [String] = []
        for sentence in Self.split(text, after: ".!?।。？！") {
            if try tokenize(sentence, special: false).count <= Self.pieceTokens {
                result.append(sentence)
                continue
            }
            for clause in Self.split(sentence, after: ",;:，；：") {
                if try tokenize(clause, special: false).count <= Self.pieceTokens {
                    result.append(clause)
                    continue
                }
                var run: [Substring] = []
                for word in clause.split(whereSeparator: \.isWhitespace) {
                    let candidate = (run + [word]).joined(separator: " ")
                    if !run.isEmpty, try tokenize(candidate, special: false).count > Self.pieceTokens {
                        result.append(run.joined(separator: " "))
                        run = [word]
                    } else {
                        run.append(word)
                    }
                }
                if !run.isEmpty {
                    result.append(run.joined(separator: " "))
                }
            }
        }
        return result
    }

    /// Splits after any of `marks` when whitespace follows, keeping each mark with its text.
    static func split(_ text: String, after marks: String) -> [String] {
        var parts: [String] = []
        var current = ""
        var previous: Character?
        for character in text {
            if character.isWhitespace, let previous, marks.contains(previous) {
                let part = current.trimmingCharacters(in: .whitespaces)
                if !part.isEmpty {
                    parts.append(part)
                }
                current = ""
            } else {
                current.append(character)
            }
            previous = character
        }
        let last = current.trimmingCharacters(in: .whitespaces)
        if !last.isEmpty {
            parts.append(last)
        }
        return parts
    }

    /// `special` also adds the end-of-sequence token the encoder expects and parses `<2xx>`.
    private func tokenize(_ text: String, special: Bool) throws -> [llama_token] {
        let byteCount = text.utf8.count
        var tokens = [llama_token](repeating: 0, count: byteCount + 8)
        let count = llama_tokenize(vocab, text, Int32(byteCount), &tokens, Int32(tokens.count), special, true)
        guard count >= 0 else { throw EngineError("the text could not be tokenized") }
        return Array(tokens.prefix(Int(count)))
    }

    private func piece(_ token: llama_token) -> String {
        var buffer = [CChar](repeating: 0, count: 64)
        let length = llama_token_to_piece(vocab, token, &buffer, Int32(buffer.count), 0, true)
        guard length > 0 else { return "" }
        return String(decoding: buffer[0..<Int(length)].map { UInt8(bitPattern: $0) }, as: UTF8.self)
    }
}
