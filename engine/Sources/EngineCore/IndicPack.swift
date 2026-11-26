// FilePath: engine/Sources/EngineCore/IndicPack.swift
// The Indic (Hinglish) model: Bodhan AI's Indic-Transcribe-Flex in FluidAudio's Canary Core ML
// layout. Flex has its own 7,152-piece tokenizer, so its prompt ids are not Canary-1B-v2's.

import CoreML
import Foundation

public enum IndicPack {
    /// Flex's canary2 prompt for Hindi with `<|romanized|>`: the model itself writes Hindi in
    /// Roman letters and keeps English words as they are (Hinglish). Ids: `<|startofcontext|>`,
    /// `<|startoftranscript|>`, `<|emo:undefined|>`, `<|hi|>` (source and target), `<|pnc|>`,
    /// `<|noitn|>`, `<|romanized|>`, `<|notimestamp|>`, `<|nodiarize|>`.
    public static let hinglishPrompt: [Int32] = [7, 4, 18, 89, 89, 5, 9, 10, 13, 15]

    /// Pieces the prompt and FluidAudio's decoder rely on, at their Flex ids.
    private static let requiredPieces: [Int: String] = [
        3: "<|endoftext|>", 4: "<|startoftranscript|>", 5: "<|pnc|>", 7: "<|startofcontext|>", 9: "<|noitn|>",
        10: "<|romanized|>", 13: "<|notimestamp|>", 15: "<|nodiarize|>", 18: "<|emo:undefined|>", 89: "<|hi|>",
    ]

    /// Fewest pieces a vocabulary can have and still hold every prompt token.
    public static let minimumPieces = (requiredPieces.keys.max() ?? 0) + 1

    /// Checks that `vocab.json` is the id -> piece object FluidAudio reads (it force-casts, so
    /// anything else would crash the helper) and that it is Flex's tokenizer: every prompt token
    /// sits at the id the prompt uses.
    public static func validateVocabulary(_ data: Data) throws {
        let json = try JSONSerialization.jsonObject(with: data)
        guard let pieces = json as? [String: String] else {
            throw Failure(description: "vocab.json is not a JSON object of id -> piece")
        }
        for (id, piece) in requiredPieces.sorted(by: { $0.key < $1.key }) where pieces[String(id)] != piece {
            throw Failure(
                description: "vocab.json has \(pieces[String(id)].map { "\"\($0)\"" } ?? "nothing") at \(id), "
                    + "not \(piece): it is not Indic-Transcribe-Flex's tokenizer")
        }
    }

    /// The four Core ML parts of the pack.
    public enum Part: Sendable {
        case preprocessor, encoder, decoder, projection
    }

    /// Where each part runs. With the Neural Engine allowed, Core ML places every op of the int4
    /// encoder on the CPU and the Neural Engine decoder is half as fast as the GPU's, so both run
    /// on the GPU, which also loads them without the Neural Engine's minute-long compile. The mel
    /// preprocessor needs float32 (its power spectrum overflows float16), so it stays on the CPU.
    public static func computeUnits(for part: Part) -> MLComputeUnits {
        switch part {
        case .preprocessor: .cpuOnly
        case .encoder, .decoder: .cpuAndGPU
        case .projection: .cpuAndNeuralEngine
        }
    }

    public struct Failure: Error, CustomStringConvertible {
        public let description: String
    }
}
