// FilePath: engine/Sources/SimpleVoiceEngine/Diarization.swift
// Who spoke when, for meeting recordings: FluidAudio's offline pipeline (pyannote Community-1
// segmentation, WeSpeaker embeddings, PLDA + VBx clustering) on the downloaded Core ML models.
// The models are loaded from the helper's own model directory only; nothing is fetched here.

import CoreML
import FluidAudio
import Foundation

struct DiarizeParams: Decodable {
    let wavPath: String
    /// Exact number of speakers, when the user knows it.
    let speakers: Int?
    let maxSpeakers: Int?
}

struct SpeakerTurn: Encodable, Sendable {
    let startMs: Int64
    let endMs: Int64
    let speaker: String
}

struct DiarizeResult: Encodable, Sendable {
    let segments: [SpeakerTurn]
    let speakerCount: Int
}

/// Keeps the diarization models loaded between meetings; each request gets its own manager
/// because the speaker-count constraint is part of its configuration.
actor SpeakerDiarizer {
    static let requiredEntries = [
        "Segmentation.mlmodelc", "FBank.mlmodelc", "Embedding.mlmodelc", "PldaRho.mlmodelc", "plda-parameters.json",
    ]

    private let store: ModelStore
    private var models: OfflineDiarizerModels?

    init(store: ModelStore) {
        self.store = store
    }

    func preload() async throws {
        _ = try await loaded()
    }

    func unload() {
        models = nil
    }

    func diarize(_ params: DiarizeParams, progress: ProgressSink) async throws -> DiarizeResult {
        let url = URL(fileURLWithPath: params.wavPath)
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw EngineError("audio file not found: \(params.wavPath)")
        }
        let models = try await loaded()
        var config = OfflineDiarizerConfig.default
        if let exact = params.speakers, exact > 0 {
            config = config.withSpeakers(exactly: exact)
        } else if let maximum = params.maxSpeakers, maximum > 0 {
            config = config.withSpeakers(min: nil, max: maximum)
        }
        await progress.report(0, "Finding speakers", force: true)
        let result = try await Self.run(config: config, models: models, url: url, progress: progress)
        try Task.checkCancellation()
        await progress.report(1, "Finished", force: true)
        return Self.turns(result)
    }

    private static func run(
        config: OfflineDiarizerConfig,
        models: OfflineDiarizerModels,
        url: URL,
        progress: ProgressSink
    ) async throws -> DiarizationResult {
        let manager = OfflineDiarizerManager(config: config)
        manager.initialize(models: models)
        return try await manager.process(url) { done, total in
            guard total > 0 else { return }
            let fraction = 0.95 * Double(done) / Double(total)
            Task { await progress.report(fraction, "Finding speakers (\(done)/\(total))") }
        }
    }

    /// Speakers are renamed S1, S2, … in order of first appearance, which is stable for the
    /// same audio and readable before the user names them.
    static func turns(_ result: DiarizationResult) -> DiarizeResult {
        let ordered = result.segments.sorted { $0.startTimeSeconds < $1.startTimeSeconds }
        var names: [String: String] = [:]
        var turns: [SpeakerTurn] = []
        turns.reserveCapacity(ordered.count)
        for segment in ordered where segment.endTimeSeconds > segment.startTimeSeconds {
            let name: String
            if let known = names[segment.speakerId] {
                name = known
            } else {
                name = "S\(names.count + 1)"
                names[segment.speakerId] = name
            }
            turns.append(
                SpeakerTurn(
                    startMs: Timeline.milliseconds(TimeInterval(segment.startTimeSeconds)),
                    endMs: Timeline.milliseconds(TimeInterval(segment.endTimeSeconds)),
                    speaker: name
                ))
        }
        return DiarizeResult(segments: turns, speakerCount: names.count)
    }

    private func loaded() async throws -> OfflineDiarizerModels {
        if let models { return models }
        guard let hub = HubModel.forModel(.speakerDiarization) else {
            throw EngineError("speaker-diarization has no downloadable files")
        }
        let directory = try store.readyDirectory(for: hub)
        let loaded = try Self.load(from: directory)
        models = loaded
        return loaded
    }

    /// The same compute units FluidAudio picks (FBank on CPU, the rest anywhere), but from the
    /// files ModelStore verified, never through FluidAudio's own downloader.
    private static func load(from directory: URL) throws -> OfflineDiarizerModels {
        let started = Date()
        let anywhere = MLModelConfiguration()
        anywhere.computeUnits = .all
        let cpu = MLModelConfiguration()
        cpu.computeUnits = .cpuOnly
        func model(_ name: String, _ configuration: MLModelConfiguration) throws -> MLModel {
            try MLModel(
                contentsOf: directory.appendingPathComponent(name, isDirectory: true), configuration: configuration)
        }
        let models = OfflineDiarizerModels(
            segmentationModel: try model("Segmentation.mlmodelc", anywhere),
            fbankModel: try model("FBank.mlmodelc", cpu),
            embeddingModel: try model("Embedding.mlmodelc", anywhere),
            pldaRhoModel: try model("PldaRho.mlmodelc", anywhere),
            pldaPsi: try pldaPsi(directory.appendingPathComponent("plda-parameters.json")),
            compilationDuration: Date().timeIntervalSince(started)
        )
        Log.info("loaded speaker-diarization in \(String(format: "%.2f", Date().timeIntervalSince(started))) s")
        return models
    }

    private struct PldaFile: Decodable {
        struct Tensor: Decodable {
            let dataBase64: String

            enum CodingKeys: String, CodingKey {
                case dataBase64 = "data_base64"
            }
        }

        let tensors: [String: Tensor]
    }

    /// The PLDA eigenvalues (`psi`): little-endian Float32 values, base64 in the JSON file.
    private static func pldaPsi(_ url: URL) throws -> [Double] {
        let file = try JSONDecoder().decode(PldaFile.self, from: Data(contentsOf: url))
        guard let encoded = file.tensors["psi"],
            let data = Data(base64Encoded: encoded.dataBase64, options: [.ignoreUnknownCharacters]),
            data.count >= MemoryLayout<Float>.size
        else {
            throw EngineError("speaker-diarization is damaged (no PLDA parameters); delete it and download it again")
        }
        let count = data.count / MemoryLayout<Float>.size
        return data.withUnsafeBytes { raw in
            (0..<count).map { index in
                Double(
                    Float(
                        bitPattern: UInt32(littleEndian: raw.loadUnaligned(fromByteOffset: index * 4, as: UInt32.self)))
                )
            }
        }
    }
}
