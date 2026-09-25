// FilePath: engine/Sources/SimpleVoiceEngine/SystemServices.swift
// Frontmost app, synthetic Cmd+V, System Settings panes and output muting.

import AppKit
import ApplicationServices
import AudioToolbox
import CoreAudio
import Foundation

enum SystemServices {
    /// NSWorkspace updates `frontmostApplication` from notifications delivered on the main run
    /// loop, which main.swift keeps running; reading it there returns the current app.
    @MainActor
    static func frontmostApp() -> FrontmostAppResult {
        let app = NSWorkspace.shared.frontmostApplication
        return FrontmostAppResult(name: app?.localizedName, bundleId: app?.bundleIdentifier)
    }

    /// Posts Cmd+V to whichever app has focus. Requires Accessibility trust.
    static func paste() async throws {
        guard await AccessibilityTrust.isTrusted() else {
            throw EngineError("accessibility permission missing")
        }
        let vKey: CGKeyCode = 9
        let source = CGEventSource(stateID: .combinedSessionState)
        guard let down = CGEvent(keyboardEventSource: source, virtualKey: vKey, keyDown: true),
            let up = CGEvent(keyboardEventSource: source, virtualKey: vKey, keyDown: false)
        else {
            throw EngineError("cannot create the Cmd+V keyboard events")
        }
        down.flags = .maskCommand
        up.flags = .maskCommand
        down.post(tap: .cghidEventTap)
        // Some apps drop a key-up that arrives in the same instant as its key-down.
        try await Task.sleep(for: .milliseconds(20))
        up.post(tap: .cghidEventTap)
    }

    @MainActor
    static func openSettings(_ kind: PermissionKind) throws {
        let anchor =
            switch kind {
            case .microphone: "Privacy_Microphone"
            case .accessibility: "Privacy_Accessibility"
            case .speech: "Privacy_SpeechRecognition"
            }
        let text = "x-apple.systempreferences:com.apple.preference.security?\(anchor)"
        guard let url = URL(string: text), NSWorkspace.shared.open(url) else {
            throw EngineError("cannot open System Settings (\(text))")
        }
    }
}

/// Mutes the default output device while dictating. Devices without a mute control (some USB
/// and HDMI outputs) are silenced by setting their volume to zero; the volume is restored on
/// unmute.
actor OutputMuter {
    private var savedVolumes: [AudioDeviceID: Float32] = [:]

    /// Returns whether the device was muted before this call.
    func setMuted(_ muted: Bool) throws -> Bool {
        let device = try Self.defaultOutputDevice()
        var muteAddress = Self.address(kAudioDevicePropertyMute)
        if Self.isSettable(device, &muteAddress) {
            let previous: UInt32 = try Self.read(device, &muteAddress)
            var value: UInt32 = muted ? 1 : 0
            try Self.write(device, &muteAddress, &value)
            return previous != 0
        }

        var volumeAddress = Self.address(kAudioHardwareServiceDeviceProperty_VirtualMainVolume)
        guard Self.isSettable(device, &volumeAddress) else {
            throw EngineError("the current output device cannot be muted")
        }
        let volume: Float32 = try Self.read(device, &volumeAddress)
        let wasMuted = volume == 0
        if muted {
            if !wasMuted {
                savedVolumes[device] = volume
                var zero: Float32 = 0
                try Self.write(device, &volumeAddress, &zero)
            }
        } else if var saved = savedVolumes.removeValue(forKey: device) {
            try Self.write(device, &volumeAddress, &saved)
        }
        return wasMuted
    }

    private static func address(_ selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress(
            mSelector: selector,
            mScope: kAudioDevicePropertyScopeOutput,
            mElement: kAudioObjectPropertyElementMain
        )
    }

    private static func defaultOutputDevice() throws -> AudioDeviceID {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let device: AudioDeviceID = try read(AudioObjectID(kAudioObjectSystemObject), &address)
        guard device != kAudioObjectUnknown else {
            throw EngineError("no default audio output device")
        }
        return device
    }

    private static func isSettable(_ object: AudioObjectID, _ address: inout AudioObjectPropertyAddress) -> Bool {
        guard AudioObjectHasProperty(object, &address) else { return false }
        var settable: DarwinBoolean = false
        let status = AudioObjectIsPropertySettable(object, &address, &settable)
        return status == noErr && settable.boolValue
    }

    private static func read<T: BitwiseCopyable>(
        _ object: AudioObjectID,
        _ address: inout AudioObjectPropertyAddress
    ) throws -> T {
        var size = UInt32(MemoryLayout<T>.size)
        let value = UnsafeMutablePointer<T>.allocate(capacity: 1)
        defer { value.deallocate() }
        let status = AudioObjectGetPropertyData(object, &address, 0, nil, &size, value)
        guard status == noErr else {
            throw EngineError("reading audio property \(address.mSelector) failed (OSStatus \(status))")
        }
        return value.pointee
    }

    private static func write<T: BitwiseCopyable>(
        _ object: AudioObjectID,
        _ address: inout AudioObjectPropertyAddress,
        _ value: inout T
    ) throws {
        let size = UInt32(MemoryLayout<T>.size)
        let status = AudioObjectSetPropertyData(object, &address, 0, nil, size, &value)
        guard status == noErr else {
            throw EngineError("writing audio property \(address.mSelector) failed (OSStatus \(status))")
        }
    }
}
