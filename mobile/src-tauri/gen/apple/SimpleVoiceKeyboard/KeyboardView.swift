// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceKeyboard/KeyboardView.swift
// The keyboard's root view: the top bar over the key rows. It is the controller's input view so
// that it can adopt UIInputViewAudioFeedback; the system plays key clicks only for input views
// that ask for them (and only with Full Access).

import UIKit

final class KeyboardView: UIInputView, UIInputViewAudioFeedback {
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
        let bar = CGRect(x: 0, y: Self.padding, width: bounds.width, height: barHeight)
        topBar?.frame = bar
        keys?.frame = CGRect(
            x: 0,
            y: bar.maxY,
            width: bounds.width,
            height: max(bounds.height - bar.maxY - Self.padding, 0)
        )
    }
}
