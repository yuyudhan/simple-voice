// FilePath: engine/Sources/SimpleVoiceEngine/Permissions.swift
// Microphone, Accessibility and Speech Recognition permission status and prompts.
//
// The helper runs as a child of Simple Voice.app, so TCC attributes every check and prompt to the
// app bundle: granting "Simple Voice" in System Settings is what these calls observe.

import AVFoundation
import ApplicationServices
import Foundation
import Speech

enum Permissions {
    static func current() async -> PermissionsResult {
        let accessibility: PermissionState = await AccessibilityTrust.isTrusted() ? .granted : .denied
        return PermissionsResult(microphone: microphone(), accessibility: accessibility, speech: speech())
    }

    /// Shows the system prompt when the user has not decided yet; otherwise reports the state.
    static func request(_ kind: PermissionKind) async -> PermissionsResult {
        switch kind {
        case .microphone:
            if AVCaptureDevice.authorizationStatus(for: .audio) == .notDetermined {
                _ = await AVCaptureDevice.requestAccess(for: .audio)
            }
        case .accessibility:
            // The key is spelled out because the imported `kAXTrustedCheckOptionPrompt` global is
            // a mutable C variable, which strict concurrency rejects.
            let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
            _ = AXIsProcessTrustedWithOptions(options)
        case .speech:
            if SFSpeechRecognizer.authorizationStatus() == .notDetermined {
                await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                    SFSpeechRecognizer.requestAuthorization { _ in
                        continuation.resume()
                    }
                }
            }
        }
        return await current()
    }

    private static func microphone() -> PermissionState {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            .granted
        case .denied:
            .denied
        case .restricted:
            .restricted
        case .notDetermined:
            .notDetermined
        @unknown default:
            .denied
        }
    }

    private static func speech() -> PermissionState {
        switch SFSpeechRecognizer.authorizationStatus() {
        case .authorized:
            .granted
        case .denied:
            .denied
        case .restricted:
            .restricted
        case .notDetermined:
            .notDetermined
        @unknown default:
            .denied
        }
    }
}
