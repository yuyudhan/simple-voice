// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceKeyboard/LevelBars.swift
// Meter bars, Signal's unit for sound (docs/internal/design.md): the last few microphone levels
// as rounded vertical bars, newest on the right, so the speaker sees they are being heard.

import UIKit

final class LevelBars: UIView {
    var colour: UIColor = .systemOrange {
        didSet { setNeedsDisplay() }
    }

    private var levels = [CGFloat](repeating: 0, count: LevelBars.bars)

    private static let bars = 5
    private static let minHeight: CGFloat = 0.2

    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = false
        contentMode = .redraw
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    /// Adds a level from 0 (silence) to 1 (full scale).
    func push(_ level: Float) {
        levels.removeFirst()
        levels.append(CGFloat(min(max(level, 0), 1)))
        setNeedsDisplay()
    }

    func clear() {
        levels = [CGFloat](repeating: 0, count: Self.bars)
        setNeedsDisplay()
    }

    override func draw(_ rect: CGRect) {
        let count = CGFloat(Self.bars)
        let gap = bounds.width / (count * 3)
        let barWidth = (bounds.width - gap * (count - 1)) / count
        colour.setFill()
        for (index, level) in levels.enumerated() {
            // Speech sits far below full scale; the square root lifts it into view.
            let height = (Self.minHeight + (1 - Self.minHeight) * level.squareRoot()) * bounds.height
            let bar = CGRect(
                x: CGFloat(index) * (barWidth + gap),
                y: (bounds.height - height) / 2,
                width: barWidth,
                height: height
            )
            UIBezierPath(roundedRect: bar, cornerRadius: barWidth / 2).fill()
        }
    }
}
