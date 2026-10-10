// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceWidget/SessionLiveActivity.swift
// The session's Live Activity (MB-7): on the Lock Screen and in the Dynamic Island it says
// the microphone is held for Simple Voice, how long is left, and offers Stop.

import ActivityKit
import SwiftUI
import WidgetKit

@main
struct SimpleVoiceWidgets: WidgetBundle {
    var body: some Widget {
        SessionLiveActivity()
    }
}

struct SessionLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: SessionActivityAttributes.self) { context in
            LockScreenView(state: context.state)
                .activityBackgroundTint(Signal.background)
                .activitySystemActionForegroundColor(Signal.text)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Image(systemName: context.state.recordingSince == nil ? "mic.fill" : "waveform")
                        .foregroundStyle(Signal.accent)
                        .font(.title2)
                }
                DynamicIslandExpandedRegion(.center) {
                    StateText(state: context.state)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    StopButton(iconOnly: true)
                }
            } compactLeading: {
                Image(systemName: context.state.recordingSince == nil ? "mic.fill" : "waveform")
                    .foregroundStyle(Signal.accent)
            } compactTrailing: {
                Clock(state: context.state)
                    .frame(maxWidth: 44)
                    .monospacedDigit()
            } minimal: {
                Image(systemName: "mic.fill")
                    .foregroundStyle(Signal.accent)
            }
        }
    }
}

private struct LockScreenView: View {
    let state: SessionActivityAttributes.ContentState

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: state.recordingSince == nil ? "mic.fill" : "waveform")
                .font(.title2)
                .foregroundStyle(Signal.accent)
                .frame(width: 36, height: 36)
            StateText(state: state)
            Spacer(minLength: 8)
            StopButton(iconOnly: false)
        }
        .padding(16)
    }
}

/// "Simple Voice · ready" with the time left, or "Recording" with the time spoken.
private struct StateText: View {
    let state: SessionActivityAttributes.ContentState

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(state.recordingSince == nil ? "Simple Voice · ready" : "Recording · \(state.model)")
                .font(.headline)
                .foregroundStyle(Signal.text)
            // A timer Text fills its width, so "left" is joined to it rather than laid beside it.
            (Clock.text(state) + Text(state.recordingSince == nil ? " left" : ""))
                .monospacedDigit()
                .font(.subheadline)
                .foregroundStyle(Signal.muted)
        }
    }
}

/// Counts down to the session's end, or up from the start of a recording.
private struct Clock: View {
    let state: SessionActivityAttributes.ContentState

    var body: some View {
        Self.text(state)
    }

    static func text(_ state: SessionActivityAttributes.ContentState) -> Text {
        if let since = state.recordingSince {
            let start = Date(timeIntervalSince1970: since / 1000)
            return Text(timerInterval: start ... start.addingTimeInterval(3600), countsDown: false)
        }
        let ends = Date(timeIntervalSince1970: state.endsAt / 1000)
        return Text(timerInterval: Date() ... max(ends, Date()), countsDown: true)
    }
}

/// Ends the session. The Dynamic Island's side region is narrow, so there it is a round icon.
private struct StopButton: View {
    let iconOnly: Bool

    var body: some View {
        Button(intent: EndSessionIntent()) {
            if iconOnly {
                Image(systemName: "stop.fill")
                    .font(.body.weight(.semibold))
                    .frame(width: 30, height: 30)
            } else {
                Label("Stop", systemImage: "stop.fill")
                    .font(.subheadline.weight(.semibold))
            }
        }
        .foregroundStyle(Signal.onAccent)
        .buttonStyle(.borderedProminent)
        .buttonBorderShape(iconOnly ? .circle : .capsule)
        .tint(Signal.accent)
        .accessibilityLabel("End dictation session")
    }
}

/// Signal tokens (src/styles/tokens.css), dark variants: the Lock Screen backdrop is dark.
private enum Signal {
    static let background = Color(red: 0x11 / 255, green: 0x11 / 255, blue: 0x13 / 255)
    static let text = Color(red: 0xED / 255, green: 0xED / 255, blue: 0xF0 / 255)
    static let muted = Color(red: 0xA2 / 255, green: 0xA2 / 255, blue: 0xAB / 255)
    static let accent = Color(red: 0xFF / 255, green: 0x6A / 255, blue: 0x2E / 255)
    static let onAccent = Color(red: 0x1A / 255, green: 0x0B / 255, blue: 0x04 / 255)
}
