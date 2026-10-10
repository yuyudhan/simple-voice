// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceWidget/EndSessionIntent.swift
// The Live Activity's Stop button. A LiveActivityIntent runs in the app's process, where the
// session listens for this Darwin notification. Compiled into the app and the widget; the app
// target cannot import SvShared, so the name is spelled out (Bridge.Note.endSession).

import AppIntents
import Foundation

struct EndSessionIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "End dictation session"
    static let isDiscoverable = false

    func perform() async throws -> some IntentResult {
        let center = CFNotificationCenterGetDarwinNotifyCenter()
        let name = CFNotificationName("dev.yuyudhan.simplevoice.endSession" as CFString)
        CFNotificationCenterPostNotification(center, name, nil, nil, true)
        return .result()
    }
}
