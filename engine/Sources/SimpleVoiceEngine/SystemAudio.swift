// FilePath: engine/Sources/SimpleVoiceEngine/SystemAudio.swift
// The other side of a meeting: everything the Mac plays, captured with ScreenCaptureKit and
// written as a 16 kHz mono PCM16 WAV next to the app's microphone recording. Only explicit
// `system_audio_start` / `system_audio_stop` requests capture; nothing else does.
//
// The file timeline is wall time: sample 0 is `startedAtMs`, and silence (macOS sends no
// buffers while nothing plays) is written as zeros, so the app can line it up with the
// microphone track by time alone.

import AVFoundation
import CoreGraphics
import CoreMedia
import Foundation
import ScreenCaptureKit

struct SystemAudioStartParams: Decodable {
    let wavPath: String
}

struct SystemAudioStartResult: Encodable, Sendable {
    let startedAtMs: Int64
}

struct SystemAudioStopResult: Encodable, Sendable {
    let durationMs: Int64
}

private struct SystemAudioFailedLine: Encodable, Sendable {
    let event = "system_audio_failed"
    let reason: String
}

enum ScreenCapturePermission {
    static let missing = "screen recording permission missing"

    /// Whether a request in this helper's lifetime was turned down; macOS offers no way to tell
    /// "denied" from "never asked" otherwise.
    nonisolated(unsafe) private static var refused = false
    private static let lock = NSLock()

    static func state() -> PermissionState {
        if CGPreflightScreenCaptureAccess() {
            return .granted
        }
        return lock.withLock { refused } ? .denied : .notDetermined
    }

    /// Shows the system prompt the first time; afterwards macOS only points to System Settings.
    static func request() {
        let granted = CGRequestScreenCaptureAccess()
        lock.withLock { refused = !granted }
    }
}

/// At most one system-audio capture runs at a time.
actor SystemAudioCapture {
    private let output: Output
    private var running: RunningCapture?

    init(output: Output) {
        self.output = output
    }

    func start(_ params: SystemAudioStartParams) async throws -> SystemAudioStartResult {
        guard running == nil else {
            throw EngineError("system audio is already being recorded")
        }
        guard CGPreflightScreenCaptureAccess() else {
            throw EngineError(ScreenCapturePermission.missing)
        }
        let content: SCShareableContent
        do {
            content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        } catch {
            throw EngineError("\(ScreenCapturePermission.missing) (\(describe(error)))")
        }
        guard let display = content.displays.first else {
            throw EngineError("no display to attach the system audio capture to")
        }
        let url = URL(fileURLWithPath: params.wavPath)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let writer = try SystemAudioWriter(url: url)
        let output = output
        let capture = RunningCapture(writer: writer) { reason in
            Log.error("system audio capture stopped: \(reason)")
            Task { await output.event(SystemAudioFailedLine(reason: reason)) }
        }

        let configuration = SCStreamConfiguration()
        configuration.capturesAudio = true
        configuration.sampleRate = Int(AudioLoader.sampleRate)
        configuration.channelCount = 1
        configuration.excludesCurrentProcessAudio = true
        // ScreenCaptureKit always attaches a video stream; the smallest, slowest one is discarded.
        configuration.width = 2
        configuration.height = 2
        configuration.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        configuration.queueDepth = 3

        let filter = SCContentFilter(display: display, excludingApplications: [], exceptingWindows: [])
        let stream = SCStream(filter: filter, configuration: configuration, delegate: capture)
        try stream.addStreamOutput(capture, type: .audio, sampleHandlerQueue: capture.queue)
        try stream.addStreamOutput(capture, type: .screen, sampleHandlerQueue: capture.queue)
        let startedAt = Date()
        capture.queue.sync { writer.begin(hostSeconds: CMClockGetTime(CMClockGetHostTimeClock()).seconds) }
        do {
            try await stream.startCapture()
        } catch {
            capture.queue.sync { writer.close() }
            try? FileManager.default.removeItem(at: url)
            throw EngineError("system audio capture failed to start: \(describe(error))")
        }
        capture.stream = stream
        running = capture
        Log.info("system audio capture started")
        return SystemAudioStartResult(startedAtMs: Int64((startedAt.timeIntervalSince1970 * 1000).rounded()))
    }

    func stop() async throws -> SystemAudioStopResult {
        guard let capture = running else {
            throw EngineError("system audio is not being recorded")
        }
        running = nil
        if let stream = capture.stream {
            do {
                try await stream.stopCapture()
            } catch {
                // Already stopped by the system (reported as `system_audio_failed`); the file
                // still holds everything captured until then.
                Log.error("stopping system audio capture: \(describe(error))")
            }
        }
        let now = CMClockGetTime(CMClockGetHostTimeClock()).seconds
        let frames = capture.queue.sync { () -> Int64 in
            capture.writer.finish(untilHostSeconds: now)
        }
        Log.info("system audio capture stopped (\(frames) frames)")
        return SystemAudioStopResult(durationMs: frames * 1000 / Int64(AudioLoader.sampleRate))
    }
}

/// Receives ScreenCaptureKit buffers on its own serial queue and hands audio to the writer.
private final class RunningCapture: NSObject, SCStreamOutput, SCStreamDelegate, @unchecked Sendable {
    let queue = DispatchQueue(label: "dev.yuyudhan.simplevoice.system-audio")
    let writer: SystemAudioWriter
    var stream: SCStream?
    private let onFailure: @Sendable (String) -> Void

    init(writer: SystemAudioWriter, onFailure: @escaping @Sendable (String) -> Void) {
        self.writer = writer
        self.onFailure = onFailure
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio, sampleBuffer.isValid else { return }
        do {
            try writer.append(sampleBuffer)
        } catch {
            onFailure("cannot write system audio: \(describe(error))")
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: any Error) {
        onFailure(describe(error))
    }
}

/// Writes 16 kHz mono PCM16, filling gaps between buffers with silence by presentation time.
/// Used only on the capture queue.
private final class SystemAudioWriter: @unchecked Sendable {
    /// Gaps shorter than this are jitter, not silence.
    private static let gapTolerance: Int64 = 1_600
    private static let silenceBlock = 16_000

    private var file: AVAudioFile?
    private let target: AVAudioFormat
    private var converter: AVAudioConverter?
    private var converterSource: AVAudioFormat?
    private var startHostSeconds = 0.0
    private(set) var written: Int64 = 0

    init(url: URL) throws {
        guard let target = AudioLoader.targetFormat else {
            throw EngineError("cannot describe 16 kHz mono audio")
        }
        self.target = target
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatLinearPCM,
            AVSampleRateKey: AudioLoader.sampleRate,
            AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
        ]
        do {
            file = try AVAudioFile(
                forWriting: url, settings: settings, commonFormat: .pcmFormatFloat32, interleaved: false)
        } catch {
            throw EngineError("cannot create \(url.lastPathComponent): \(describe(error))")
        }
    }

    func begin(hostSeconds: Double) {
        startHostSeconds = hostSeconds
        written = 0
    }

    func append(_ sampleBuffer: CMSampleBuffer) throws {
        guard file != nil else { return }
        let source = try Self.pcmBuffer(sampleBuffer)
        let converted = try convert(source)
        let presentation = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
        if presentation.isNumeric {
            let expected = Int64(((presentation.seconds - startHostSeconds) * AudioLoader.sampleRate).rounded())
            if expected - written > Self.gapTolerance {
                try writeSilence(expected - written)
            }
        }
        try write(converted)
    }

    /// Pads silence up to the stop time, closes the file and returns the frames written.
    func finish(untilHostSeconds now: Double) -> Int64 {
        let expected = Int64(((now - startHostSeconds) * AudioLoader.sampleRate).rounded())
        if expected - written > Self.gapTolerance {
            do {
                try writeSilence(expected - written)
            } catch {
                Log.error("padding system audio failed: \(describe(error))")
            }
        }
        close()
        return written
    }

    func close() {
        file = nil
    }

    private func write(_ buffer: AVAudioPCMBuffer) throws {
        guard let file, buffer.frameLength > 0 else { return }
        try file.write(from: buffer)
        written += Int64(buffer.frameLength)
    }

    private func writeSilence(_ frames: Int64) throws {
        var remaining = frames
        while remaining > 0 {
            let count = Int(min(remaining, Int64(Self.silenceBlock)))
            try write(try AudioLoader.makeBuffer([Float](repeating: 0, count: count)))
            remaining -= Int64(count)
        }
    }

    private func convert(_ input: AVAudioPCMBuffer) throws -> AVAudioPCMBuffer {
        if input.format == target {
            return input
        }
        if converterSource != input.format {
            guard let made = AVAudioConverter(from: input.format, to: target) else {
                throw EngineError("unsupported system audio format: \(input.format)")
            }
            converter = made
            converterSource = input.format
        }
        guard let converter else { throw EngineError("no system audio converter") }
        let ratio = target.sampleRate / input.format.sampleRate
        let capacity = AVAudioFrameCount((Double(input.frameLength) * ratio).rounded(.up)) + 256
        guard let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else {
            throw EngineError("cannot allocate a conversion buffer")
        }
        let feeder = SingleBufferFeeder(input, afterwards: .noDataNow)
        var conversionError: NSError?
        let status = converter.convert(to: output, error: &conversionError) { _, inputStatus in
            feeder.next(inputStatus)
        }
        if status == .error {
            throw EngineError("system audio conversion failed: \(conversionError.map(describe) ?? "unknown error")")
        }
        return output
    }

    private static func pcmBuffer(_ sampleBuffer: CMSampleBuffer) throws -> AVAudioPCMBuffer {
        guard let description = CMSampleBufferGetFormatDescription(sampleBuffer),
            let stream = CMAudioFormatDescriptionGetStreamBasicDescription(description),
            let format = AVAudioFormat(streamDescription: stream)
        else {
            throw EngineError("system audio buffer has no audio format")
        }
        let frames = AVAudioFrameCount(CMSampleBufferGetNumSamples(sampleBuffer))
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames) else {
            throw EngineError("cannot allocate a system audio buffer")
        }
        buffer.frameLength = frames
        let status = CMSampleBufferCopyPCMDataIntoAudioBufferList(
            sampleBuffer, at: 0, frameCount: Int32(frames), into: buffer.mutableAudioBufferList)
        guard status == noErr else {
            throw EngineError("cannot read system audio (OSStatus \(status))")
        }
        return buffer
    }
}
