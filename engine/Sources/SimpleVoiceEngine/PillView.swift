// FilePath: engine/Sources/SimpleVoiceEngine/PillView.swift
// The pill itself, a small graphite instrument: an on-air light, a level meter and a mono readout.
// Its palette is fixed because it floats over arbitrary apps in either appearance, so it ignores
// the app theme (the one exception in docs/internal/design.md).

import Observation
import SwiftUI

@MainActor
@Observable
final class PillModel {
    var phase: PillPhase = .idle
    var sessionId: UInt64 = 0
    var startedAt = Date()
    var text = ""
    var note: String?
    var message: String?
    /// The session edits the selected text; the pill then reads as an edit, not a dictation.
    var edit = false
    /// Read on every frame by the level bars, so it is deliberately not observed.
    @ObservationIgnored let meter = LevelMeter()
}

/// Turns the ~30 Hz level stream into smooth bar heights, one step per displayed frame.
@MainActor
final class LevelMeter {
    static let barCount = 11
    private static let attack = 0.45
    private static let decay = 0.12
    /// Centre bars reach higher than the edges, like a voice envelope.
    private static let profile: [Double] = (0..<barCount).map { index in
        let x = (Double(index) - Double(barCount - 1) / 2) / (Double(barCount - 1) / 2)
        return 0.4 + 0.6 * cos(x * .pi / 2)
    }

    /// RMS level of the latest audio block, 0...1.
    var level = 0.0
    private var heights = [Double](repeating: 0, count: barCount)
    private var lastTime: TimeInterval?

    func reset() {
        level = 0
        heights = [Double](repeating: 0, count: Self.barCount)
        lastTime = nil
    }

    /// Eases every bar toward the current level and returns the heights (0...1). The easing is
    /// scaled to 60 Hz frames, so a 120 Hz display moves the bars at the same speed.
    func advance(to time: TimeInterval) -> [Double] {
        let frames = lastTime.map { min(max((time - $0) * 60, 0), 4) } ?? 1
        lastTime = time
        // Perceived loudness is closer to the square root of the RMS level.
        let target = min(1, level.squareRoot() * 1.25)
        for index in heights.indices {
            let wobble = 0.72 + 0.28 * sin(time * 1000 / (140 + Double(index) * 23) + Double(index) * 1.7)
            let desired = target * Self.profile[index] * wobble
            let rate = desired > heights[index] ? Self.attack : Self.decay
            heights[index] += (desired - heights[index]) * (1 - pow(1 - rate, frames))
        }
        return heights
    }
}

enum PillStyle {
    /// Wide enough for the longest pill (an 80-character preview) plus its shadow.
    static let windowSize = CGSize(width: 580, height: 56)
    /// The pill hugs a finished dictation's text up to this width.
    static let maxPillWidth: CGFloat = 560
    static let ease = Animation.timingCurve(0.25, 0.8, 0.25, 1, duration: 0.2)

    static let background = rgb(0x1C1C1F, 0.94)
    static let rim = rgb(0x000000, 0.55)
    static let edge = rgb(0xFFFFFF, 0.09)
    static let well = rgb(0xFFFFFF, 0.06)
    static let text = rgb(0xF3F1ED)
    static let muted = rgb(0xF3F1ED, 0.6)
    static let dimOpacity = 0.26
    static let mutedOpacity = 0.6
    static let signal = rgb(0xFF6A2E)
    static let signalGlow = rgb(0xFF6A2E, 0.55)
    static let signalHalo = rgb(0xFF6A2E, 0.2)
    static let success = rgb(0x5FD08F)
    static let successWell = rgb(0x5FD08F, 0.16)
    static let danger = rgb(0xFF6B7A)
    static let dangerWell = rgb(0xFF6B7A, 0.16)
    static let dangerText = rgb(0xFFB3BB)
    static let warning = rgb(0xF0C05A)
    static let warningWell = rgb(0xF0C05A, 0.16)
    static let shadow = rgb(0x000000, 0.3)

    private static func rgb(_ hex: UInt32, _ opacity: Double = 1) -> Color {
        Color(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: opacity
        )
    }
}

struct PillView: View {
    let model: PillModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private static let barMin = 3.0
    private static let barMax = 22.0
    /// Bars above this share of their range are the loudest and light up in signal orange.
    private static let hotLevel = 0.62
    /// The pill's widest text: its maximum width less the glyph, the gap and the padding.
    private static let maxTextWidth = PillStyle.maxPillWidth - 8 - 22 - 9 - 13

    var body: some View {
        let phase = model.phase
        TimelineView(.animation(paused: !Self.isAnimated(phase))) { timeline in
            pill(phase, at: timeline.date.timeIntervalSinceReferenceDate)
        }
        .frame(width: PillStyle.windowSize.width, height: PillStyle.windowSize.height)
    }

    private static func isAnimated(_ phase: PillPhase) -> Bool {
        phase == .recording || phase == .transcribing || phase == .formatting
    }

    private func pill(_ phase: PillPhase, at time: TimeInterval) -> some View {
        HStack(spacing: 9) {
            glyph(phase, at: time)
            content(phase, at: time)
            trailing(phase, at: time)
        }
        .padding(.leading, 8)
        .padding(.trailing, 13)
        // A finished dictation sizes the pill to its text (never narrower than the others);
        // every other phase has a fixed width.
        .frame(width: Self.fixedWidth(phase), height: 38)
        .frame(minWidth: phase == .done ? 188 : nil, alignment: .leading)
        .fixedSize(horizontal: phase == .done, vertical: false)
        .background {
            Capsule()
                .fill(PillStyle.background)
                .overlay(Capsule().strokeBorder(PillStyle.edge, lineWidth: 1))
                .overlay(Capsule().stroke(PillStyle.rim, lineWidth: 0.5))
                .compositingGroup()
                .shadow(color: PillStyle.shadow, radius: 3, y: 2)
        }
        .foregroundStyle(PillStyle.text)
        .font(.system(size: 12))
        .opacity(phase == .idle ? 0.8 : phase == .cancelled ? 0 : 1)
        .scaleEffect(phase == .cancelled ? 0.96 : 1)
    }

    private static func fixedWidth(_ phase: PillPhase) -> CGFloat? {
        switch phase {
        case .idle: 104
        case .done: nil
        // "TRANSCRIBING" and the level bars need a little more than the recording timer.
        case .transcribing, .formatting: 200
        default: 188
        }
    }

    // MARK: - On-air light

    private func glyph(_ phase: PillPhase, at time: TimeInterval) -> some View {
        let well: Color
        let tint: Color
        switch phase {
        case .done where model.note != nil:
            // Pasted, but the formatting pass fell back ("unformatted").
            (well, tint) = (PillStyle.warningWell, PillStyle.warning)
        case .done:
            (well, tint) = (PillStyle.successWell, PillStyle.success)
        case .error:
            (well, tint) = (PillStyle.dangerWell, PillStyle.danger)
        default:
            (well, tint) = (PillStyle.well, PillStyle.muted)
        }
        return ZStack {
            Circle().fill(well)
            Circle().strokeBorder(PillStyle.edge, lineWidth: 1)
            glyphSymbol(phase, at: time)
        }
        .foregroundStyle(tint)
        .frame(width: 22, height: 22)
        .animation(.timingCurve(0.25, 0.8, 0.25, 1, duration: 0.15), value: phase)
    }

    @ViewBuilder
    private func glyphSymbol(_ phase: PillPhase, at time: TimeInterval) -> some View {
        switch phase {
        // An edit keeps its pen from the first word to the result, so it never reads as dictation.
        case .recording where model.edit, .transcribing where model.edit, .formatting where model.edit:
            Image(systemName: "pencil").font(.system(size: 11, weight: .semibold))
        case .recording:
            ZStack {
                Circle().fill(PillStyle.signalHalo).frame(width: 14, height: 14)
                Circle().fill(PillStyle.signal).frame(width: 8, height: 8)
                    .shadow(color: PillStyle.signalGlow, radius: 4)
            }
            // The on-air light breathes between full and 55 % every 1.6 s.
            .opacity(reduceMotion ? 1 : 0.775 + 0.225 * cos(2 * .pi * time / 1.6))
        case .done:
            Image(systemName: "checkmark").font(.system(size: 10, weight: .heavy))
        case .error:
            Image(systemName: "exclamationmark.triangle").font(.system(size: 10, weight: .semibold))
        default:
            Image(systemName: "mic").font(.system(size: 10, weight: .semibold))
        }
    }

    // MARK: - Body

    @ViewBuilder
    private func content(_ phase: PillPhase, at time: TimeInterval) -> some View {
        switch phase {
        case .done:
            Text(model.edit ? "Edited" : model.text)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: Self.maxTextWidth, alignment: .leading)
        case .error:
            Text(model.message ?? "Dictation failed")
                .font(.system(size: 11.5))
                .foregroundStyle(PillStyle.dangerText)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
        default:
            bars(phase, at: time)
        }
    }

    private func bars(_ phase: PillPhase, at time: TimeInterval) -> some View {
        let levels = phase == .recording ? model.meter.advance(to: time) : []
        return HStack(spacing: 2) {
            ForEach(0..<LevelMeter.barCount, id: \.self) { index in
                let bar = Self.bar(index, phase, levels, at: time)
                RoundedRectangle(cornerRadius: 1.5)
                    .fill(bar.color)
                    .frame(width: 3, height: bar.height)
            }
        }
        .frame(maxWidth: .infinity)
        .frame(height: 24)
    }

    private static func bar(
        _ index: Int, _ phase: PillPhase, _ levels: [Double], at time: TimeInterval
    ) -> (height: Double, color: Color) {
        switch phase {
        case .recording:
            let level = levels.indices.contains(index) ? levels[index] : 0
            return (barMin + level * (barMax - barMin), level > hotLevel ? PillStyle.signal : PillStyle.text)
        case .transcribing, .formatting:
            // A wave runs across the bars while the text is being produced.
            let cycle = 1.2
            let offset = (time - Double(index) * 0.08).truncatingRemainder(dividingBy: cycle)
            let progress = (offset < 0 ? offset + cycle : offset) / cycle
            let rise: Double
            if progress < 0.35 {
                rise = smoothstep(progress / 0.35)
            } else if progress < 0.7 {
                rise = smoothstep(1 - (progress - 0.35) / 0.35)
            } else {
                rise = 0
            }
            let opacity = PillStyle.dimOpacity + (PillStyle.mutedOpacity - PillStyle.dimOpacity) * rise
            return (4 + 6 * rise, PillStyle.text.opacity(opacity))
        default:
            return (barMin, PillStyle.text.opacity(PillStyle.dimOpacity))
        }
    }

    private static func smoothstep(_ value: Double) -> Double {
        value * value * (3 - 2 * value)
    }

    // MARK: - Trailing readout

    @ViewBuilder
    private func trailing(_ phase: PillPhase, at time: TimeInterval) -> some View {
        switch phase {
        case .recording:
            Text(Self.elapsed(from: model.startedAt.timeIntervalSinceReferenceDate, to: time))
                .font(.system(size: 11.5, design: .monospaced))
                .monospacedDigit()
                .foregroundStyle(PillStyle.muted)
                .frame(minWidth: 30, alignment: .trailing)
        case .transcribing, .formatting:
            Text(phase == .transcribing ? "TRANSCRIBING" : model.edit ? "EDITING" : "FORMATTING")
                .font(.system(size: 9.5, weight: .medium, design: .monospaced))
                .tracking(0.57)
                .foregroundStyle(PillStyle.muted)
                // A label is never wrapped: the level bars give way instead (they are centred
                // in whatever room is left).
                .lineLimit(1)
                .fixedSize()
                .layoutPriority(1)
        default:
            EmptyView()
        }
    }

    private static func elapsed(from start: TimeInterval, to now: TimeInterval) -> String {
        let total = max(0, Int(now - start))
        let seconds = total % 60
        return "\(total / 60):\(seconds < 10 ? "0" : "")\(seconds)"
    }
}
