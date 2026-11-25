// FilePath: engine/Tests/EngineCoreChecks/SplitEncoderChecks.swift
// Fluid Speech Pico's encoder pieces run through `SplitEncoder`. FluidAudio calls Core ML's async
// `prediction`, whose base implementation needs the model's own engine, which a subclass lacks:
// without SplitEncoder's override the helper crashed (SIGSEGV) on the first Pico transcription,
// and this check crashes the same way.

import CoreML
import EngineCore
import Foundation

/// A stand-in encoder piece: copies its input (`mel` for the first piece, `hidden_in` after) to
/// `hidden_out`, adding one, so the check sees every piece ran in order.
private final class FakePiece: MLModel, @unchecked Sendable {
    override func prediction(
        from input: any MLFeatureProvider, options: MLPredictionOptions
    ) throws -> any MLFeatureProvider {
        guard let value = (input.featureValue(for: "hidden_in") ?? input.featureValue(for: "mel"))?.multiArrayValue
        else {
            throw SplitEncoder.Failure(description: "fake piece got neither mel nor hidden_in")
        }
        let out = try MLMultiArray(shape: value.shape, dataType: .float32)
        for index in 0..<value.count {
            out[index] = NSNumber(value: value[index].floatValue + 1)
        }
        return try MLDictionaryFeatureProvider(dictionary: ["hidden_out": out])
    }
}

@MainActor func checkSplitEncoder() async {
    do {
        let encoder = try SplitEncoder(pieces: [FakePiece(), FakePiece(), FakePiece(), FakePiece()])
        let mel = try MLMultiArray(shape: [1, 2], dataType: .float32)
        mel[0] = 1
        mel[1] = 5
        let length = try MLMultiArray(shape: [1], dataType: .int32)
        length[0] = 2
        let input = try MLDictionaryFeatureProvider(dictionary: ["mel": mel, "mel_length": length])

        // The async form FluidAudio uses (Core ML's completion-handler entry point).
        let model: MLModel = encoder
        let output = try await model.prediction(from: input, options: MLPredictionOptions())
        let hidden = output.featureValue(for: "hidden_out")?.multiArrayValue
        check(
            hidden.map { [$0[0].floatValue, $0[1].floatValue] } == [5, 9],
            "the async prediction runs all four pieces in order")

        let sync = try syncPrediction(encoder, input)
        check(sync.featureValue(for: "hidden_out")?.multiArrayValue?[0].floatValue == 5, "sync prediction too")

        do {
            _ = try await model.prediction(
                from: try MLDictionaryFeatureProvider(dictionary: ["mel": mel]), options: MLPredictionOptions())
            check(false, "an input without mel_length must throw")
        } catch {
            check("\(error)".contains("mel_length"), "the error names mel_length: \(error)")
        }
    } catch {
        check(false, "split encoder checks threw \(error)")
    }
}

/// The synchronous entry point; in an async context Swift would pick the async overload.
private func syncPrediction(_ model: MLModel, _ input: any MLFeatureProvider) throws -> any MLFeatureProvider {
    try model.prediction(from: input)
}