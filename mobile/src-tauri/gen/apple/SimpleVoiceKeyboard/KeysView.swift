// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceKeyboard/KeysView.swift
// The key rows. Each layer's keys are built the first time it shows and then kept, hidden while
// another layer is up, so typing only updates captions and colours and never rebuilds views
// (keyboard extensions get a few tens of megabytes).

import UIKit

final class KeysView: UIView {
    private weak var delegate: KeyButtonDelegate?
    /// Receives the globe key's touches for `handleInputModeList(from:with:)`.
    private weak var inputController: UIInputViewController?

    private var palette: Palette
    private var shownLayer = Layer.letters
    private var shift = ShiftState.off
    private var returnAction = ReturnAction.newline
    private var showsGlobe = true
    private var built: [Layer: [[KeyButton]]] = [:]

    init(palette: Palette, delegate: KeyButtonDelegate, inputController: UIInputViewController) {
        self.palette = palette
        self.delegate = delegate
        self.inputController = inputController
        super.init(frame: .zero)
        showLayer()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func set(layer: Layer, shift: ShiftState) {
        guard layer != shownLayer || shift != self.shift else { return }
        let changed = layer != shownLayer
        shownLayer = layer
        self.shift = shift
        if changed { showLayer() } else { updateKeys() }
    }

    func set(returnAction: ReturnAction) {
        guard returnAction != self.returnAction else { return }
        self.returnAction = returnAction
        updateKeys()
    }

    /// The globe key shows only where the system has no switcher of its own.
    func set(showsGlobe: Bool) {
        guard showsGlobe != self.showsGlobe else { return }
        self.showsGlobe = showsGlobe
        for rows in built.values {
            for button in rows.joined() where button.key == .nextKeyboard { button.isHidden = !showsGlobe }
        }
        setNeedsLayout()
    }

    func apply(_ palette: Palette) {
        self.palette = palette
        for rows in built.values {
            for button in rows.joined() { button.apply(palette) }
        }
    }

    /// Releases any key still held, e.g. backspace repeating as the keyboard hides.
    func finishTouches() {
        for button in current.joined() { button.finishTouch() }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let rows = current
        guard !rows.isEmpty else { return }
        let rowHeight = bounds.height / CGFloat(rows.count)
        for (index, row) in rows.enumerated() {
            let visible = row.filter { !$0.isHidden }
            // The middle letter row is narrower, as on every phone keyboard.
            let inset: CGFloat = shownLayer == .letters && row.count == 9 ? 0.5 : 0
            let units = visible.reduce(inset * 2) { $0 + $1.key.weight }
            let unit = bounds.width / units
            var x = inset * unit
            let y = CGFloat(index) * rowHeight
            for button in visible {
                let width = button.key.weight * unit
                button.frame = CGRect(x: x, y: y, width: width, height: rowHeight)
                x += width
            }
        }
    }

    private var current: [[KeyButton]] { built[shownLayer] ?? [] }

    private func showLayer() {
        if built[shownLayer] == nil { built[shownLayer] = build(shownLayer) }
        for (layer, rows) in built {
            let hidden = layer != shownLayer
            for button in rows.joined() {
                button.isHidden = hidden || (button.key == .nextKeyboard && !showsGlobe)
            }
        }
        updateKeys()
        setNeedsLayout()
    }

    private func updateKeys() {
        for button in current.joined() { button.update(shift: shift, returnAction: returnAction) }
    }

    private func build(_ layer: Layer) -> [[KeyButton]] {
        KeyLayout.rows(layer).map { row in
            row.map { key in
                let button = KeyButton(key: key, palette: palette)
                button.delegate = delegate
                if key == .nextKeyboard, let inputController {
                    button.addTarget(
                        inputController,
                        action: #selector(UIInputViewController.handleInputModeList(from:with:)),
                        for: .allTouchEvents
                    )
                }
                addSubview(button)
                return button
            }
        }
    }
}
