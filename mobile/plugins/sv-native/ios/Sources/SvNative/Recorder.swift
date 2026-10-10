// FilePath: mobile/plugins/sv-native/ios/Sources/SvNative/Recorder.swift
// The session's microphone. iOS lets an app keep recording in the background only if it
// started in the foreground, so the engine starts with the session and runs until it ends;
// a dictation just switches writing on and off. Audio arrives on the engine's thread and is
// written as 16 kHz mono PCM16 WAV, the format Groq Whisper and the desktop app use.

import AVFoundation
import Foundation

enum RecorderError: LocalizedError {
    case format
    case file(String)

    var errorDescription: String? {
        switch self {
        case .format: return "The microphone's audio format is not supported"
        case let .file(reason): return "Could not save the recording: \(reason)"
        }
    }
}

final class Recorder {
    private let engine = AVAudioEngine()
    private let lock = NSLock()
    private var writer: WavWriter?
    private var converter: AVAudioConverter?
    private var peak: Float = 0
    private(set) var running = false

    /// The loudest sample since the last read, 0 to 1.
    var level: Float {
        lock.lock()
        defer { lock.unlock() }
        let value = peak
        peak = 0
        return value
    }

    var isRecording: Bool {
        lock.lock()
        defer { lock.unlock() }
        return writer != nil
    }

    /// Starts the audio session and the engine. Must run while the app is in the foreground.
    func start() throws {
        guard !running else { return }
        let audio = AVAudioSession.sharedInstance()
        // Mixing keeps music playing; the app holds the microphone, not the speaker.
        try audio.setCategory(.playAndRecord, mode: .default, options: [.mixWithOthers, .allowBluetoothHFP, .defaultToSpeaker])
        try audio.setActive(true)
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard let target = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 16_000, channels: 1, interleaved: true),
              let converter = AVAudioConverter(from: format, to: target)
        else { throw RecorderError.format }
        self.converter = converter
        input.installTap(onBus: 0, bufferSize: 2048, format: format) { [weak self] buffer, _ in
            self?.receive(buffer)
        }
        engine.prepare()
        try engine.start()
        running = true
    }

    /// Stops the engine and gives the microphone back.
    func stop() {
        _ = finish()
        guard running else { return }
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        running = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    func begin(to url: URL) throws {
        let next = try WavWriter(url: url)
        lock.lock()
        writer = next
        lock.unlock()
    }

    /// Ends the current recording: its file and length in milliseconds, or nil if none.
    func finish() -> (url: URL, durationMs: Int64)? {
        lock.lock()
        let current = writer
        writer = nil
        lock.unlock()
        guard let current else { return nil }
        current.close()
        return (current.url, current.durationMs)
    }

    /// Ends the current recording and deletes it.
    func discard() {
        if let file = finish() {
            try? FileManager.default.removeItem(at: file.url)
        }
    }

    private func receive(_ buffer: AVAudioPCMBuffer) {
        let loudest = Self.peak(of: buffer)
        lock.lock()
        peak = max(peak, loudest)
        let current = writer
        lock.unlock()
        guard let current, let converter else { return }
        let ratio = converter.outputFormat.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 16
        guard let out = AVAudioPCMBuffer(pcmFormat: converter.outputFormat, frameCapacity: capacity) else { return }
        var fed = false
        var error: NSError?
        converter.convert(to: out, error: &error) { _, status in
            if fed {
                status.pointee = .noDataNow
                return nil
            }
            fed = true
            status.pointee = .haveData
            return buffer
        }
        if error == nil { current.append(out) }
    }

    private static func peak(of buffer: AVAudioPCMBuffer) -> Float {
        guard let channel = buffer.floatChannelData?[0] else { return 0 }
        var loudest: Float = 0
        for index in 0 ..< Int(buffer.frameLength) {
            loudest = max(loudest, abs(channel[index]))
        }
        return min(loudest, 1)
    }
}

/// A WAV file whose header is written once its length is known.
final class WavWriter {
    let url: URL
    private let handle: FileHandle
    private var samples: Int64 = 0
    private static let headerBytes = 44
    private static let sampleRate: Int64 = 16_000

    init(url: URL) throws {
        self.url = url
        let manager = FileManager.default
        try? manager.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        guard manager.createFile(atPath: url.path, contents: Data(count: Self.headerBytes)) else {
            throw RecorderError.file(url.lastPathComponent)
        }
        handle = try FileHandle(forWritingTo: url)
        try handle.seekToEnd()
    }

    var durationMs: Int64 { samples * 1000 / Self.sampleRate }

    func append(_ buffer: AVAudioPCMBuffer) {
        guard let data = buffer.int16ChannelData?[0] else { return }
        let count = Int(buffer.frameLength)
        handle.write(Data(bytes: data, count: count * 2))
        samples += Int64(count)
    }

    func close() {
        try? handle.seek(toOffset: 0)
        handle.write(header())
        try? handle.close()
    }

    private func header() -> Data {
        var data = Data()
        func text(_ value: String) { data.append(contentsOf: Array(value.utf8)) }
        func int32(_ value: Int64) { withUnsafeBytes(of: UInt32(truncatingIfNeeded: value).littleEndian) { data.append(contentsOf: $0) } }
        func int16(_ value: UInt16) { withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) } }
        let dataBytes = samples * 2
        text("RIFF")
        int32(36 + dataBytes)
        text("WAVE")
        text("fmt ")
        int32(16)
        int16(1) // PCM
        int16(1) // mono
        int32(Self.sampleRate)
        int32(Self.sampleRate * 2) // byte rate
        int16(2) // block align
        int16(16) // bits per sample
        text("data")
        int32(dataBytes)
        return data
    }
}
