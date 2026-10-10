// FilePath: mobile/plugins/sv-native/ios/Sources/SvShared/SessionActivity.swift
// The Live Activity a session shows on the Lock Screen and in the Dynamic Island. The app
// starts and updates it; the widget extension draws it. ActivityKit matches the two by this
// type's name, so the same file is compiled into both.

#if canImport(ActivityKit)
import ActivityKit
import Foundation

public struct SessionActivityAttributes: ActivityAttributes {
    public struct ContentState: Codable, Hashable {
        /// Milliseconds since 1970 when the session ends on its own.
        public var endsAt: Double
        /// Set while recording: milliseconds since 1970 when the recording started.
        public var recordingSince: Double?
        /// The model a dictation uses now, e.g. "Groq Whisper".
        public var model: String

        public init(endsAt: Double, recordingSince: Double?, model: String) {
            self.endsAt = endsAt
            self.recordingSince = recordingSince
            self.model = model
        }
    }

    public init() {}
}
#endif
