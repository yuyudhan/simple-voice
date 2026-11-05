// FilePath: engine/Sources/SimpleVoiceEngine/SystemServices.swift
// Frontmost app, synthetic Cmd+V and Cmd+C, System Settings panes and output muting.

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
        try await postCommandKey(9)
    }

    /// Posts Cmd+<key> to whichever app has focus. The flags are set explicitly, so modifiers
    /// the user is still holding (the edit shortcut's Option) do not change the combination.
    static func postCommandKey(_ key: CGKeyCode) async throws {
        let source = CGEventSource(stateID: .combinedSessionState)
        guard let down = CGEvent(keyboardEventSource: source, virtualKey: key, keyDown: true),
            let up = CGEvent(keyboardEventSource: source, virtualKey: key, keyDown: false)
        else {
            throw EngineError("cannot create the synthetic keyboard events")
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
            case .systemAudio: "Privacy_ScreenCapture"
            }
        let text = "x-apple.systempreferences:com.apple.preference.security?\(anchor)"
        guard let url = URL(string: text), NSWorkspace.shared.open(url) else {
            throw EngineError("cannot open System Settings (\(text))")
        }
    }
}

/// Mutes the default output device while dictating, which silences every app and browser tab at
/// once without pausing them, and unmutes exactly that device afterwards. The device is found
/// again by its UID, so a default output that changed meanwhile (AirPods connected) is left
/// alone. Devices without a mute control (some USB and HDMI outputs) are silenced by setting
/// their volume to zero; the app keeps that volume and hands it back to unmute, so a crash of
/// either process loses nothing.
actor OutputMuter {
    func mute() throws -> MuteOutputResult {
        let device = try Self.defaultOutputDevice()
        let uid = try Self.uid(of: device)
        var muteAddress = Self.address(kAudioDevicePropertyMute)
        if Self.isSettable(device, &muteAddress) {
            let previous: UInt32 = try Self.read(device, &muteAddress)
            if previous == 0 {
                var muted: UInt32 = 1
                try Self.write(device, &muteAddress, &muted)
            }
            return MuteOutputResult(device: uid, wasMuted: previous != 0, volume: nil)
        }

        var volumeAddress = Self.address(kAudioHardwareServiceDeviceProperty_VirtualMainVolume)
        guard Self.isSettable(device, &volumeAddress) else {
            throw EngineError("the current output device cannot be muted")
        }
        let volume: Float32 = try Self.read(device, &volumeAddress)
        if volume == 0 {
            return MuteOutputResult(device: uid, wasMuted: true, volume: nil)
        }
        var zero: Float32 = 0
        try Self.write(device, &volumeAddress, &zero)
        return MuteOutputResult(device: uid, wasMuted: false, volume: volume)
    }

    /// Unmutes the device `mute` muted, or puts back its volume; false when the device is gone.
    func unmute(_ params: UnmuteOutputParams) throws -> Bool {
        guard let device = Self.device(uid: params.device) else { return false }
        if var volume = params.volume {
            var volumeAddress = Self.address(kAudioHardwareServiceDeviceProperty_VirtualMainVolume)
            let current: Float32 = try Self.read(device, &volumeAddress)
            // A volume the user set during the recording stays.
            if current == 0 {
                try Self.write(device, &volumeAddress, &volume)
            }
            return true
        }
        var muteAddress = Self.address(kAudioDevicePropertyMute)
        var unmuted: UInt32 = 0
        try Self.write(device, &muteAddress, &unmuted)
        return true
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

    private static func uid(of device: AudioDeviceID) throws -> String {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyDeviceUID,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        var value: Unmanaged<CFString>?
        let status = AudioObjectGetPropertyData(device, &address, 0, nil, &size, &value)
        guard status == noErr, let uid = value?.takeRetainedValue() else {
            throw EngineError("reading the output device UID failed (OSStatus \(status))")
        }
        return uid as String
    }

    /// The device with `uid` if it is still connected.
    private static func device(uid: String) -> AudioDeviceID? {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyTranslateUIDToDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let qualifier = uid as CFString
        var device = AudioDeviceID(kAudioObjectUnknown)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        let status = withUnsafePointer(to: qualifier) { pointer in
            AudioObjectGetPropertyData(
                AudioObjectID(kAudioObjectSystemObject), &address,
                UInt32(MemoryLayout<CFString>.size), pointer, &size, &device
            )
        }
        guard status == noErr, device != kAudioObjectUnknown else { return nil }
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
