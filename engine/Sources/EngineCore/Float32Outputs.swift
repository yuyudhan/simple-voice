// FilePath: engine/Sources/EngineCore/Float32Outputs.swift
// Presents a Core ML model whose float16 outputs are handed on as float32. FluidAudio 0.17.4's
// Canary decoder reads float16 arrays through its own bit conversion, which subtracts in
// unsigned arithmetic and traps for every value under 1 in magnitude; float32 arrays take its
// plain path. The Indic model's int4 parts all return float16.

import Accelerate
import CoreML
import Foundation

public final class Float32Outputs: MLModel, @unchecked Sendable {
    private let inner: MLModel

    public init(_ inner: MLModel) {
        self.inner = inner
        super.init()
    }

    override public var modelDescription: MLModelDescription {
        inner.modelDescription
    }

    override public func prediction(from input: any MLFeatureProvider) throws -> any MLFeatureProvider {
        try prediction(from: input, options: MLPredictionOptions())
    }

    /// The async entry point; the base implementation needs an engine this subclass lacks.
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
        let output = try inner.prediction(from: input, options: options)
        var converted: [String: MLFeatureValue] = [:]
        for name in output.featureNames {
            guard let value = output.featureValue(for: name) else { continue }
            if let array = value.multiArrayValue, array.dataType == .float16 {
                converted[name] = MLFeatureValue(multiArray: try Self.float32(array))
            } else {
                converted[name] = value
            }
        }
        return try MLDictionaryFeatureProvider(dictionary: converted)
    }

    /// A dense float32 copy of a float16 array, read through its strides (Core ML pads the last
    /// dimension), so the copy is in logical order.
    static func float32(_ half: MLMultiArray) throws -> MLMultiArray {
        let shape = half.shape.map(\.intValue)
        let strides = half.strides.map(\.intValue)
        let result = try MLMultiArray(shape: half.shape, dataType: .float32)
        let source = half.dataPointer.assumingMemoryBound(to: UInt16.self)
        var target = result.dataPointer.assumingMemoryBound(to: Float.self)
        guard let rowLength = shape.last, let rowStride = strides.last, result.count > 0 else { return result }
        // One row (the last dimension) at a time; `outer` walks the other dimensions.
        var outer = [Int](repeating: 0, count: shape.count - 1)
        for _ in 0..<(result.count / rowLength) {
            var offset = 0
            for axis in outer.indices { offset += outer[axis] * strides[axis] }
            if rowStride == 1 {
                // Contiguous row: one vectorised conversion (the decoder's hidden state is
                // 256 x 1024 values on every step, too many for a scalar loop).
                var from = vImage_Buffer(
                    data: UnsafeMutableRawPointer(source + offset), height: 1, width: vImagePixelCount(rowLength),
                    rowBytes: rowLength * MemoryLayout<UInt16>.size)
                var to = vImage_Buffer(
                    data: UnsafeMutableRawPointer(target), height: 1, width: vImagePixelCount(rowLength),
                    rowBytes: rowLength * MemoryLayout<Float>.size)
                let status = vImageConvert_Planar16FtoPlanarF(&from, &to, vImage_Flags(kvImageNoFlags))
                guard status == kvImageNoError else {
                    throw Failure(description: "float16 conversion failed (vImage \(status))")
                }
            } else {
                for column in 0..<rowLength {
                    target[column] = Float(Float16(bitPattern: source[offset + column * rowStride]))
                }
            }
            target += rowLength
            var axis = outer.count - 1
            while axis >= 0 {
                outer[axis] += 1
                if outer[axis] < shape[axis] { break }
                outer[axis] = 0
                axis -= 1
            }
        }
        return result
    }

    public struct Failure: Error, CustomStringConvertible {
        public let description: String
    }
}
