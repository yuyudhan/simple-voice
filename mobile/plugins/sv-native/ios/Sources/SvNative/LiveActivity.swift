// FilePath: mobile/plugins/sv-native/ios/Sources/SvNative/LiveActivity.swift
// The session's Live Activity: it explains the orange microphone dot and gives a one-tap way
// to end the session (MB-7). Drawn by the widget extension.

import ActivityKit
import Foundation
import SvShared

final class LiveActivity {
    private var activity: Activity<SessionActivityAttributes>?

    func show(endsAt: Date, recordingSince: Date?, model: String) {
        let state = SessionActivityAttributes.ContentState(
            endsAt: endsAt.timeIntervalSince1970 * 1000,
            recordingSince: recordingSince.map { $0.timeIntervalSince1970 * 1000 },
            model: model
        )
        let content = ActivityContent(state: state, staleDate: endsAt)
        if let activity {
            Task { await activity.update(content) }
            return
        }
        guard ActivityAuthorizationInfo().areActivitiesEnabled else { return }
        activity = try? Activity.request(attributes: SessionActivityAttributes(), content: content)
    }

    func end() {
        guard let activity else { return }
        self.activity = nil
        Task { await activity.end(nil, dismissalPolicy: .immediate) }
    }
}
