// FilePath: engine/Sources/SimpleVoiceEngine/LoginItem.swift
// Launch at login through SMAppService, so Simple Voice is listed under System Settings →
// General → Login Items → Open at Login.

import Foundation
import ServiceManagement

struct LoginItemParams: Decodable {
    let enabled: Bool
}

enum LoginItemState: String, Encodable, Sendable {
    case enabled
    case disabled
    /// Registered, but the user switched it off in System Settings; only they can switch it back.
    case requiresApproval = "requires_approval"
}

struct LoginItemResult: Encodable, Sendable {
    let status: LoginItemState
}

enum LoginItem {
    static func status() throws -> LoginItemResult {
        LoginItemResult(status: state(of: try service()))
    }

    /// Registers or removes the app. A registration macOS holds for approval opens the Login
    /// Items pane, because the user has to allow it there.
    static func setEnabled(_ enabled: Bool) throws -> LoginItemResult {
        let service = try service()
        let current = state(of: service)
        if enabled, current == .disabled {
            try service.register()
        } else if !enabled, current != .disabled {
            try service.unregister()
        }
        let result = LoginItemResult(status: state(of: service))
        if enabled, result.status == .requiresApproval {
            SMAppService.openSystemSettingsLoginItems()
        }
        return result
    }

    /// `mainApp` is the bundle the running executable sits in. The helper lives in the app's
    /// Contents/MacOS, so that is Simple Voice.app; a development build is a bare binary with no
    /// app around it and must not register whatever directory it happens to be in.
    private static func service() throws -> SMAppService {
        guard Bundle.main.bundleURL.pathExtension == "app" else {
            throw EngineError("Launch at login only works in the installed Simple Voice.app")
        }
        return SMAppService.mainApp
    }

    private static func state(of service: SMAppService) -> LoginItemState {
        switch service.status {
        case .enabled:
            return .enabled
        case .requiresApproval:
            return .requiresApproval
        case .notRegistered, .notFound:
            return .disabled
        @unknown default:
            return .disabled
        }
    }
}
