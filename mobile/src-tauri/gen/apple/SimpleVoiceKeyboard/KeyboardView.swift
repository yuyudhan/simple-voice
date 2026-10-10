// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceKeyboard/KeyboardView.swift
// The keyboard's content: the top bar over the key rows. It sits inside the input view iOS gives
// the controller, pinned to its edges; the controller sizes that view with one height constraint
// (Apple's documented way). Replacing the input view itself, or letting it size itself, makes
// iOS re-measure and grow the keyboard on every layout pass.
// It still adopts UIInputViewAudioFeedback, so key clicks play where the system allows them.

import UIKit

final class KeyboardView: UIView, UIInputViewAudioFeedback {
    private var topBar: TopBar?
    private var keys: KeysView?

    /// Points of the top bar and of each key row; landscape phones get a shorter keyboard.
    private var barHeight: CGFloat { traitCollection.verticalSizeClass == .compact ? 38 : 44 }
    private var rowHeight: CGFloat { traitCollection.verticalSizeClass == .compact ? 40 : 52 }
    private static let padding: CGFloat = 4

    var enableInputClicksWhenVisible: Bool { true }

    /// The height the keyboard asks for, for the controller's height constraint.
    var preferredHeight: CGFloat { Self.padding * 2 + barHeight + rowHeight * 4 }

    func embed(topBar: TopBar, keys: KeysView) {
        self.topBar = topBar
        self.keys = keys
        addSubview(topBar)
        addSubview(keys)
        setNeedsLayout()
    }

    func apply(_ palette: Palette) {
        backgroundColor = palette.background
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        // Rows keep their fixed height under the top bar even when the system hands the view
        // more room than it asked for (part of it sits behind the system's globe and mic bar),
        // so keys never stretch.
        let bar = CGRect(x: 0, y: Self.padding, width: bounds.width, height: barHeight)
        topBar?.frame = bar
        keys?.frame = CGRect(x: 0, y: bar.maxY, width: bounds.width, height: rowHeight * 4)
    }
}
