// FilePath: engine/Sources/SimpleVoiceEngine/InputDevices.swift
// The microphone side of Core Audio: whether any input device exists, what the default input is
// called, and an `input_devices_changed` event whenever either changes, so the app notices a
// microphone plugged in or pulled out right away instead of at the next failed recording.

import AVFoundation
import CoreAudio
import Foundation

/// Why a live stream got no audio, in the app's `CaptureProblem` JSON shape: a `problem` tag plus
/// that problem's fields. Only the problems the helper can tell apart on its own are modelled.
enum CaptureProblem: Encodable, Sendable, Equatable {
    case noAudio(device: String)
    case permissionDenied
    case noDevice

    private enum CodingKeys: String, CodingKey {
        case problem
        case device
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .noAudio(let device):
            try container.encode("no_audio", forKey: .problem)
            try container.encode(device, forKey: .device)
        case .permissionDenied:
            try container.encode("permission_denied", forKey: .problem)
        case .noDevice:
            try container.encode("no_device", forKey: .problem)
        }
    }
}

/// Unsolicited, so it carries no request id.
private struct InputDevicesChangedLine: Encodable, Sendable {
    let event = "input_devices_changed"
}

enum InputDevices {
    /// Core Audio reports one plug as several callbacks (the device list, then the default input,
    /// sometimes twice); changes this close together become a single event.
    static let changeDebounce: Duration = .milliseconds(300)

    /// The most specific reason no audio reached a live stream: microphone access is off, there
    /// is nothing to record from, or the default input delivered nothing.
    static func noAudioProblem() -> CaptureProblem {
        let access = AVCaptureDevice.authorizationStatus(for: .audio)
        if access == .denied || access == .restricted {
            return .permissionDenied
        }
        guard hasInputDevice() else { return .noDevice }
        return .noAudio(device: defaultInputName() ?? "the microphone")
    }

    /// The name of the default input device as System Settings shows it, or nil when there is none.
    private static func defaultInputName() -> String? {
        guard let device = defaultInputDevice() else { return nil }
        var address = globalAddress(kAudioObjectPropertyName)
        var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        var value: Unmanaged<CFString>?
        let status = AudioObjectGetPropertyData(device, &address, 0, nil, &size, &value)
        guard status == noErr, let name = value?.takeRetainedValue() else { return nil }
        let text = name as String
        return text.isEmpty ? nil : text
    }

    /// Whether any connected device can record: an input device is one with input streams.
    private static func hasInputDevice() -> Bool {
        allDevices().contains(where: hasInputStreams)
    }

    /// Emits `input_devices_changed` on every change to the device list or the default input,
    /// for the life of the process. Call once at launch.
    static func watchChanges(output: Output) {
        let coalescer = ChangeCoalescer(output: output)
        let queue = DispatchQueue(label: "simple-voice.input-devices")
        let listener: AudioObjectPropertyListenerBlock = { _, _ in
            Task { await coalescer.signal() }
        }
        for selector in [kAudioHardwarePropertyDevices, kAudioHardwarePropertyDefaultInputDevice] {
            var address = globalAddress(selector)
            let status = AudioObjectAddPropertyListenerBlock(
                AudioObjectID(kAudioObjectSystemObject), &address, queue, listener)
            if status != noErr {
                Log.error("cannot watch audio input property \(selector) (OSStatus \(status))")
            }
        }
    }

    private static func defaultInputDevice() -> AudioDeviceID? {
        var address = globalAddress(kAudioHardwarePropertyDefaultInputDevice)
        var device = AudioDeviceID(kAudioObjectUnknown)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        let status = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &device)
        guard status == noErr, device != kAudioObjectUnknown else { return nil }
        return device
    }

    private static func allDevices() -> [AudioDeviceID] {
        let system = AudioObjectID(kAudioObjectSystemObject)
        var address = globalAddress(kAudioHardwarePropertyDevices)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(system, &address, 0, nil, &size) == noErr else { return [] }
        let stride = MemoryLayout<AudioDeviceID>.size
        var devices = [AudioDeviceID](repeating: kAudioObjectUnknown, count: Int(size) / stride)
        guard !devices.isEmpty else { return [] }
        guard AudioObjectGetPropertyData(system, &address, 0, nil, &size, &devices) == noErr else { return [] }
        // The list can shrink between the size query and the read; `size` holds what was written.
        return Array(devices.prefix(Int(size) / stride))
    }

    private static func hasInputStreams(_ device: AudioDeviceID) -> Bool {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyStreams,
            mScope: kAudioObjectPropertyScopeInput,
            mElement: kAudioObjectPropertyElementMain
        )
        var size: UInt32 = 0
        let status = AudioObjectGetPropertyDataSize(device, &address, 0, nil, &size)
        return status == noErr && size > 0
    }

    private static func globalAddress(_ selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress(
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
    }
}

/// Restarts a short timer on every Core Audio callback and writes one event when it expires.
private actor ChangeCoalescer {
    private let output: Output
    private var pending: Task<Void, Never>?

    init(output: Output) {
        self.output = output
    }

    func signal() {
        pending?.cancel()
        let output = output
        pending = Task {
            do {
                try await Task.sleep(for: InputDevices.changeDebounce)
            } catch {
                return
            }
            await output.event(InputDevicesChangedLine())
        }
    }
}
