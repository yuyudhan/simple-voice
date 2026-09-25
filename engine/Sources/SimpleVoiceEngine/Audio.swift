// FilePath: engine/Sources/SimpleVoiceEngine/Audio.swift
// Loads the app's dictation WAVs as the 16 kHz mono Float32 samples every local model expects.

import AVFoundation
import Foundation

enum AudioLoader {
    static let sampleRate: Double = 16_000

    static var targetFormat: AVAudioFormat? {
        AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: sampleRate, channels: 1, interleaved: false)
    }

    /// Reads any PCM WAV and returns 16 kHz mono Float32 samples in -1...1.
    static func loadSamples(path: String) throws -> [Float] {
        let url = URL(fileURLWithPath: path)
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw EngineError("audio file not found: \(path)")
        }
        let file: AVAudioFile
        do {
            file = try AVAudioFile(forReading: url, commonFormat: .pcmFormatFloat32, interleaved: false)
        } catch {
            throw EngineError("cannot read audio file \(url.lastPathComponent): \(describe(error))")
        }
        let format = file.processingFormat
        let frameCount = AVAudioFrameCount(file.length)
        guard frameCount > 0 else { return [] }
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frameCount) else {
            throw EngineError("cannot allocate an audio buffer for \(frameCount) frames")
        }
        try file.read(into: buffer)

        if format.sampleRate == sampleRate, format.channelCount == 1 {
            return samples(of: buffer, channel: 0)
        }
        return try convert(buffer)
    }

    /// Wraps samples in a 16 kHz mono buffer for APIs that consume `AVAudioPCMBuffer`.
    static func makeBuffer(_ samples: [Float]) throws -> AVAudioPCMBuffer {
        guard let format = targetFormat,
            let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(samples.count)),
            let channel = buffer.floatChannelData?[0]
        else {
            throw EngineError("cannot allocate a 16 kHz audio buffer")
        }
        samples.withUnsafeBufferPointer { source in
            if let base = source.baseAddress {
                channel.update(from: base, count: samples.count)
            }
        }
        buffer.frameLength = AVAudioFrameCount(samples.count)
        return buffer
    }

    private static func samples(of buffer: AVAudioPCMBuffer, channel: Int) -> [Float] {
        guard let data = buffer.floatChannelData?[channel] else { return [] }
        return Array(UnsafeBufferPointer(start: data, count: Int(buffer.frameLength)))
    }

    /// Downmixes and resamples with AVAudioConverter; only reached for files that do not already
    /// match the 16 kHz mono format the app records.
    private static func convert(_ input: AVAudioPCMBuffer) throws -> [Float] {
        guard let target = targetFormat, let converter = AVAudioConverter(from: input.format, to: target) else {
            throw EngineError("unsupported audio format: \(input.format)")
        }
        let ratio = sampleRate / input.format.sampleRate
        let capacity = AVAudioFrameCount((Double(input.frameLength) * ratio).rounded(.up)) + 1_024
        guard let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else {
            throw EngineError("cannot allocate a conversion buffer")
        }
        let feeder = SingleBufferFeeder(input)
        var conversionError: NSError?
        let status = converter.convert(to: output, error: &conversionError) { _, inputStatus in
            feeder.next(inputStatus)
        }
        if status == .error {
            throw EngineError("audio conversion failed: \(conversionError.map(describe) ?? "unknown error")")
        }
        return samples(of: output, channel: 0)
    }
}

/// Hands the converter the whole input once, then reports end of stream. The converter calls the
/// input block synchronously on the calling thread, so the lock only satisfies Sendable checking.
private final class SingleBufferFeeder: @unchecked Sendable {
    private let buffer: AVAudioPCMBuffer
    private let lock = NSLock()
    private var delivered = false

    init(_ buffer: AVAudioPCMBuffer) {
        self.buffer = buffer
    }

    func next(_ status: UnsafeMutablePointer<AVAudioConverterInputStatus>) -> AVAudioBuffer? {
        lock.lock()
        defer { lock.unlock() }
        if delivered {
            status.pointee = .endOfStream
            return nil
        }
        delivered = true
        status.pointee = .haveData
        return buffer
    }
}
