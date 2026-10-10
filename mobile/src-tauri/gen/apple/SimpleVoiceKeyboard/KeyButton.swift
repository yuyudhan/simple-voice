// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceKeyboard/KeyButton.swift
// One key. Its frame tiles the row with no gaps, so a touch between two faces still lands on a
// key; the visible face sits inset inside it. Built once per layer and updated in place.

import UIKit

protocol KeyButtonDelegate: AnyObject {
    func keyTapped(_ key: Key)
    /// A long press: the corner character of a text key, or caps lock on shift.
    func keyLongPressed(_ key: Key)
    /// The finger moved `offset` cursor steps along the space bar.
    func spaceDragged(by offset: Int)
}

final class KeyButton: UIControl {
    let key: Key
    weak var delegate: KeyButtonDelegate?

    private let face = UIView()
    private let label = UILabel()
    private let icon = UIImageView()
    private let hint = UILabel()
    private var palette: Palette
    private var shift = ShiftState.off
    private var returnAction = ReturnAction.newline
    private var symbolName = ""
    private var pressed = false
    private var timer: Timer?
    private var longPressed = false
    private var dragStartX: CGFloat = 0
    private var dragSteps = 0
    private var dragging = false

    private static let symbolConfig = UIImage.SymbolConfiguration(pointSize: 18, weight: .regular)
    private static let longPressDelay: TimeInterval = 0.45
    private static let repeatDelay: TimeInterval = 0.4
    private static let repeatInterval: TimeInterval = 0.06
    /// Points of drag per cursor step, and the distance before a touch on space counts as a drag.
    private static let dragStep: CGFloat = 12
    private static let dragStart: CGFloat = 10

    init(key: Key, palette: Palette) {
        self.key = key
        self.palette = palette
        super.init(frame: .zero)
        face.isUserInteractionEnabled = false
        face.layer.cornerRadius = 6
        face.layer.cornerCurve = .continuous
        face.layer.borderWidth = 1
        addSubview(face)
        label.textAlignment = .center
        label.font = .systemFont(ofSize: key.isFunction ? 15 : 22)
        face.addSubview(label)
        icon.contentMode = .center
        face.addSubview(icon)
        if case .text(_, let alt?) = key {
            hint.text = alt
            hint.font = .systemFont(ofSize: 9, weight: .medium)
            hint.textAlignment = .right
            face.addSubview(hint)
        }
        isAccessibilityElement = true
        accessibilityTraits = .keyboardKey
        refresh()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func apply(_ palette: Palette) {
        self.palette = palette
        refresh()
    }

    func update(shift: ShiftState, returnAction: ReturnAction) {
        guard shift != self.shift || returnAction != self.returnAction else { return }
        self.shift = shift
        self.returnAction = returnAction
        refresh()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        face.frame = bounds.insetBy(dx: 3, dy: 5)
        label.frame = face.bounds
        icon.frame = face.bounds
        // Tucked into the corner, clear of the letter's glyph on narrow phone keys.
        hint.frame = CGRect(x: 0, y: 1, width: face.bounds.width - 3, height: 11)
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesBegan(touches, with: event)
        setPressed(true)
        UIDevice.current.playInputClick()
        longPressed = false
        switch key {
        case .backspace:
            delegate?.keyTapped(key)
            schedule(after: Self.repeatDelay) { [weak self] in self?.repeatBackspace() }
        case .space:
            dragStartX = touches.first?.location(in: self).x ?? 0
            dragSteps = 0
            dragging = false
        case .text(_, let alt) where alt != nil:
            schedule(after: Self.longPressDelay) { [weak self] in self?.fireLongPress() }
        case .shift:
            schedule(after: Self.longPressDelay) { [weak self] in self?.fireLongPress() }
        default:
            break
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesMoved(touches, with: event)
        guard key == .space, let touch = touches.first else { return }
        let distance = touch.location(in: self).x - dragStartX
        if !dragging && abs(distance) > Self.dragStart { dragging = true }
        let steps = Int(distance / Self.dragStep)
        if dragging && steps != dragSteps {
            delegate?.spaceDragged(by: steps - dragSteps)
            dragSteps = steps
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesEnded(touches, with: event)
        let consumed = longPressed || dragging
        finishTouch()
        // Backspace acted on touch down; the globe key is driven by the system's input-mode list.
        switch key {
        case .backspace, .nextKeyboard: return
        default: if !consumed { delegate?.keyTapped(key) }
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesCancelled(touches, with: event)
        finishTouch()
    }

    /// Stops any repeat or long-press timer, e.g. when the keyboard goes away mid-press.
    func finishTouch() {
        timer?.invalidate()
        timer = nil
        dragging = false
        longPressed = false
        setPressed(false)
    }

    private func schedule(after delay: TimeInterval, _ action: @escaping () -> Void) {
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: delay, repeats: false) { _ in action() }
    }

    private func repeatBackspace() {
        delegate?.keyTapped(key)
        timer = Timer.scheduledTimer(withTimeInterval: Self.repeatInterval, repeats: true) { [weak self] _ in
            guard let self else { return }
            self.delegate?.keyTapped(self.key)
        }
    }

    private func fireLongPress() {
        timer = nil
        longPressed = true
        UIDevice.current.playInputClick()
        delegate?.keyLongPressed(key)
    }

    private func setPressed(_ value: Bool) {
        guard pressed != value else { return }
        pressed = value
        refresh()
    }

    private func refresh() {
        // Only caps lock fills the key with the accent; one-letter shift just fills the arrow.
        let locked = key == .shift && shift == .locked
        let idle = locked ? palette.accent : key.isFunction ? palette.functionKey : palette.key
        let down = key.isFunction ? palette.key : palette.functionKey
        face.backgroundColor = pressed ? down : idle
        face.layer.borderColor = palette.border.cgColor
        label.textColor = palette.text
        hint.textColor = palette.muted.withAlphaComponent(0.7)
        icon.tintColor = locked && !pressed ? palette.onAccent : palette.text

        let name = iconName()
        if name != symbolName {
            symbolName = name
            icon.image = name.isEmpty ? nil : UIImage(systemName: name, withConfiguration: Self.symbolConfig)
        }
        label.text = caption()
        accessibilityLabel = spokenName()
    }

    private func iconName() -> String {
        switch key {
        case .shift:
            switch shift {
            case .off: return "shift"
            case .once: return "shift.fill"
            case .locked: return "capslock.fill"
            }
        case .backspace: return "delete.left"
        case .nextKeyboard: return "globe"
        case .returnKey: return returnAction.symbol
        case .text, .space, .switchLayer: return ""
        }
    }

    private func caption() -> String? {
        switch key {
        case .text(let text, _): return shift == .off ? text : text.uppercased()
        case .space: return "space"
        case .switchLayer(_, let label): return label
        case .shift, .backspace, .returnKey, .nextKeyboard: return nil
        }
    }

    private func spokenName() -> String {
        switch key {
        case .text(let text, _): return text
        case .shift:
            switch shift {
            case .off: return "Shift"
            case .once: return "Shift on"
            case .locked: return "Caps lock"
            }
        case .backspace: return "Delete"
        case .space: return "Space"
        case .returnKey: return returnAction.label
        case .nextKeyboard: return "Next keyboard"
        case .switchLayer(_, let label): return label
        }
    }
}
