// FilePath: engine/Tests/EngineCoreChecks/Float32OutputsChecks.swift
// The Indic model's int4 Core ML parts return float16 arrays, and FluidAudio 0.17.4's Canary
// decoder turns float16 bits into floats with unsigned arithmetic that overflows (a crash) for
// any value under 1 in magnitude. `Float32Outputs` hands FluidAudio float32 arrays instead.

import CoreML
import EngineCore
import Foundation

/// A stand-in Core ML part: returns a strided float16 array (rows padded from 3 to 4, as Core
/// ML pads the last dimension) holding small, negative and zero values, plus an int32 array.
private final class HalfModel: MLModel, @unchecked Sendable {
    override func prediction(
        from input: any MLFeatureProvider, options: MLPredictionOptions
    ) throws -> any MLFeatureProvider {
        let values: [Float16] = [0.25, -0.5, 0, 3, -1e-3, 65504]
        let storage = UnsafeMutablePointer<Float16>.allocate(capacity: 8)
        storage.initialize(repeating: .nan, count: 8)
        for (index, value) in values.enumerated() {
            storage[index / 3 * 4 + index % 3] = value
        }
        let half = try MLMultiArray(
            dataPointer: storage, shape: [2, 3], dataType: .float16, strides: [4, 1],
            deallocator: { $0.deallocate() })
        let count = try MLMultiArray(shape: [1], dataType: .int32)
        count[0] = 7
        return try MLDictionaryFeatureProvider(dictionary: ["decoder": half, "count": count])
    }
}

@MainActor func checkFloat32Outputs() {
    do {
        let model = Float32Outputs(HalfModel())
        let output = try model.prediction(from: MLDictionaryFeatureProvider(dictionary: [:]))
        guard let array = output.featureValue(for: "decoder")?.multiArrayValue else {
            check(false, "the float16 output is kept")
            return
        }
        check(array.dataType == .float32, "a float16 output becomes float32")
        check(array.shape == [2, 3], "the shape is kept: \(array.shape)")
        let read = (0..<6).map { array[[NSNumber(value: $0 / 3), NSNumber(value: $0 % 3)]].floatValue }
        check(read == [0.25, -0.5, 0, 3, Float(Float16(-1e-3)), 65504], "values and padding strides are kept: \(read)")
        check(
            output.featureValue(for: "count")?.multiArrayValue?.dataType == .int32,
            "a non-float16 output is passed through")
    } catch {
        check(false, "float32 output checks threw \(error)")
    }
}
