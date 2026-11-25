// FilePath: engine/Sources/SimpleVoiceEngine/FluidSpeechPack.swift
// FluidVoice's Fluid Speech packs: Parakeet TDT 0.6B v3 derivatives with the v3 tokenizer,
// preprocessor, decoder and joint contract, shipped as one tar on FluidVoice's model host. A pack's
// encoder is either one `Encoder.mlmodelc` or pieces `Encoder-1…N.mlmodelc` run back to back
// (`hidden_out` feeds the next piece's `hidden_in`, every piece takes `mel_length`). FluidAudio
// expects one encoder model, so `SplitEncoder` presents the pieces as one and the rest of the TDT
// pipeline (batch, sliding-window streaming, the booster) is v3's. Another pack is one entry in
// `forModel` plus its catalog licence and source.

@preconcurrency import CoreML
import EngineCore
import FluidAudio
import Foundation

struct FluidSpeechPack: Sendable {
    let archive: ArchiveSource

    /// `encoderPieces`: 1 for a single `Encoder.mlmodelc`, else the number of `Encoder-N` pieces.
    init(url: String, sha256: String, bytes: Int64, encoderPieces: Int) {
        archive = ArchiveSource(
            url: url,
            sha256: sha256,
            bytes: bytes,
            entries: ["Preprocessor.mlmodelc"] + Self.encoderNames(pieces: encoderPieces) + [
                "Decoder.mlmodelc", "JointDecision.mlmodelc", "parakeet_vocab.json",
            ]
        )
    }

    static func forModel(_ id: ModelID) -> FluidSpeechPack? {
        switch id {
        // Fluid Speech Pico 1.1.1 (CC-BY-4.0), from FluidVoice's model feed
        // (`models.fluidvoice.com/feed/v1/speech-models.json`).
        case .fluidSpeechPico:
            FluidSpeechPack(
                url: "https://models.fluidvoice.app/parakeet/fluid-pico/1.1.1/fluid-parakeet-pico-coreml.tar",
                sha256: "542f74f639438bf37ff134a7a75daddb50d301deb87696994e11ff272df21512",
                bytes: 161_208_320,
                encoderPieces: 4
            )
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash, .whisperLargeV3Turbo, .whisperHinglish, .indicHinglish,
            .appleSpeech, .appleIntelligence, .qwen3Instruct, .speakerDiarization, .madlad, .parakeetVocabulary:
            nil
        }
    }

    private static func encoderNames(pieces: Int) -> [String] {
        pieces <= 1 ? ["Encoder.mlmodelc"] : (1...pieces).map { "Encoder-\($0).mlmodelc" }
    }

    /// Same component placement as FluidAudio's `AsrModels.loadLocal`: the preprocessor on the
    /// CPU, everything else on the configuration's units (CPU and Neural Engine).
    static func load(from directory: URL) throws -> AsrModels {
        let configuration = AsrModels.defaultConfiguration()
        func component(_ name: String, units: MLComputeUnits) throws -> MLModel {
            let modelConfiguration = MLModelConfiguration()
            modelConfiguration.computeUnits = units
            return try MLModel(contentsOf: directory.appendingPathComponent(name), configuration: modelConfiguration)
        }
        let pieces = try encoderFiles(in: directory).map { try component($0, units: configuration.computeUnits) }
        return AsrModels(
            encoder: pieces.count == 1 ? pieces[0] : try SplitEncoder(pieces: pieces),
            preprocessor: try component("Preprocessor.mlmodelc", units: .cpuOnly),
            decoder: try component("Decoder.mlmodelc", units: configuration.computeUnits),
            joint: try component("JointDecision.mlmodelc", units: configuration.computeUnits),
            configuration: configuration,
            vocabulary: try vocabulary(at: directory.appendingPathComponent("parakeet_vocab.json")),
            version: .v3
        )
    }

    /// The pack's encoder files in run order.
    private static func encoderFiles(in directory: URL) throws -> [String] {
        let exists = { (name: String) in
            FileManager.default.fileExists(atPath: directory.appendingPathComponent(name).path)
        }
        if exists("Encoder.mlmodelc") { return ["Encoder.mlmodelc"] }
        let pieces = Array((1...).lazy.map { "Encoder-\($0).mlmodelc" }.prefix(while: exists))
        guard pieces.count > 1 else { throw EngineError("\(directory.lastPathComponent) has no encoder") }
        return pieces
    }

    /// `parakeet_vocab.json` maps token ids (as strings) to pieces; the v3 decoder needs every id
    /// below its blank.
    private static func vocabulary(at url: URL) throws -> [Int: String] {
        let pieces = try JSONDecoder().decode([String: String].self, from: Data(contentsOf: url))
        var vocabulary: [Int: String] = [:]
        for (key, piece) in pieces {
            guard let id = Int(key) else { throw EngineError("\(url.lastPathComponent) has a non-numeric id `\(key)`") }
            vocabulary[id] = piece
        }
        let blank = AsrModelVersion.v3.blankId
        guard (0..<blank).allSatisfy({ vocabulary[$0] != nil }) else {
            throw EngineError("\(url.lastPathComponent) lacks tokens below the blank id \(blank)")
        }
        return vocabulary
    }
}
