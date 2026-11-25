// FilePath: engine/Sources/EngineCore/SplitEncoder.swift
// A Fluid Speech pack's encoder shipped as several Core ML pieces (`hidden_out` -> `hidden_in`,
// each also taking `mel_length`), presented to FluidAudio as one encoder model.

import CoreML
import Foundation

/// Runs the encoder pieces as one Core ML model. FluidAudio reads the encoder's input names
/// from `modelDescription` (the first piece's) and calls the async `prediction`, which Core ML
/// sends to `predictionFromFeatures:options:completionHandler:`. Its base implementation queues
/// the request on the model's own engine, which a subclass does not have (the helper crashed
/// there), so every prediction entry point is overridden and runs the pieces synchronously.
public final class SplitEncoder: MLModel, @unchecked Sendable {
    private let pieces: [MLModel]

    public init(pieces: [MLModel]) throws {
        guard pieces.count > 1 else { throw Failure(description: "a split encoder needs at least two pieces") }
        self.pieces = pieces
        super.init()
    }

    override public var modelDescription: MLModelDescription {
        pieces[0].modelDescription
    }

    override public func prediction(from input: any MLFeatureProvider) throws -> any MLFeatureProvider {
        try prediction(from: input, options: MLPredictionOptions())
    }

    override public func __prediction(
        fromFeatures input: any MLFeatureProvider, options: MLPredictionOptions,
        completionHandler: @escaping ((any MLFeatureProvider)?, (any Error)?) -> Void
    ) {
        do {
            completionHandler(try prediction(from: input, options: options), nil)
        } catch {
            completionHandler(nil, error)
        }
    }

    override public func prediction(
        from input: any MLFeatureProvider, options: MLPredictionOptions
    ) throws -> any MLFeatureProvider {
        guard let melLength = input.featureValue(for: "mel_length") else {
            throw Failure(description: "encoder input lacks mel_length")
        }
        var output = try pieces[0].prediction(from: input, options: options)
        for piece in pieces.dropFirst() {
            guard let hidden = output.featureValue(for: "hidden_out") else {
                throw Failure(description: "an encoder piece produced no hidden_out")
            }
            let next = try MLDictionaryFeatureProvider(dictionary: ["hidden_in": hidden, "mel_length": melLength])
            output = try piece.prediction(from: next, options: options)
        }
        return output
    }

    public struct Failure: Error, CustomStringConvertible {
        public let description: String

        public init(description: String) {
            self.description = description
        }
    }
}
