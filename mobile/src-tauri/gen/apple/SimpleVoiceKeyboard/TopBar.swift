// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceKeyboard/TopBar.swift
// The row above the keys: the status (dot and text, or the level meter while recording) or the
// spelling suggestions on the left, the mic on the right. One row, as on Android.

import UIKit

/// What the status dot and text say.
enum Tone {
    case ready, blocked, recording, working, retry
}

/// The mic button: tap to start, tap to stop (MB-3), or unavailable.
enum MicState {
    case idle, disabled, recording, busy
}

protocol TopBarDelegate: AnyObject {
    func micTapped()
    func statusTapped()
    func suggestionTapped(_ word: String)
}

final class TopBar: UIView {
    private weak var delegate: TopBarDelegate?
    private var palette: Palette
    private let status = UIControl()
    private let dot = UIView()
    private let levels = LevelBars()
    private let statusText = UILabel()
    private let suggestions = UIView()
    private let cells = (0..<3).map { _ in SuggestionCell() }
    private let dividers = (0..<2).map { _ in UIView() }
    private let mic = UIButton(type: .custom)
    private var tone = Tone.ready
    private var micState = MicState.disabled
    private var words: [String] = []

    private static let micSize: CGFloat = 38
    private static let symbolConfig = UIImage.SymbolConfiguration(pointSize: 16, weight: .regular)

    init(palette: Palette, delegate: TopBarDelegate) {
        self.palette = palette
        self.delegate = delegate
        super.init(frame: .zero)
        dot.layer.cornerRadius = 3.5
        dot.isUserInteractionEnabled = false
        levels.isUserInteractionEnabled = false
        levels.isHidden = true
        statusText.font = .systemFont(ofSize: 13)
        statusText.isUserInteractionEnabled = false
        status.addSubview(dot)
        status.addSubview(levels)
        status.addSubview(statusText)
        status.isAccessibilityElement = true
        status.accessibilityTraits = .button
        status.addTarget(self, action: #selector(statusTap), for: .touchUpInside)
        addSubview(status)

        for (index, cell) in cells.enumerated() {
            cell.titleLabel?.font = index == 0 ? .boldSystemFont(ofSize: 15) : .systemFont(ofSize: 15)
            cell.titleLabel?.lineBreakMode = .byTruncatingTail
            cell.addTarget(self, action: #selector(suggestionTap(_:)), for: .touchUpInside)
            suggestions.addSubview(cell)
        }
        for divider in dividers { suggestions.addSubview(divider) }
        suggestions.isHidden = true
        addSubview(suggestions)

        mic.layer.cornerRadius = Self.micSize / 2
        mic.addTarget(self, action: #selector(micTap), for: .touchUpInside)
        addSubview(mic)
        apply(palette)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func apply(_ palette: Palette) {
        self.palette = palette
        statusText.textColor = palette.muted
        levels.colour = palette.accent
        for cell in cells {
            cell.setTitleColor(palette.text, for: .normal)
            cell.pressedColour = palette.functionKey
        }
        for divider in dividers { divider.backgroundColor = palette.border }
        setStatusColours()
        setMicLook()
    }

    func setStatus(_ line: String, tone: Tone) {
        statusText.text = line
        status.accessibilityLabel = line
        self.tone = tone
        setStatusColours()
        // While recording, the meter replaces the dot and shows the voice being heard.
        let recording = tone == .recording
        dot.isHidden = recording
        if recording == levels.isHidden {
            levels.clear()
            levels.isHidden = !recording
            setNeedsLayout()
        }
        showBar()
    }

    /// The microphone's latest level, from 0 to 1.
    func setLevel(_ level: Float) {
        levels.push(level)
    }

    func setMic(_ state: MicState) {
        guard state != micState else { return }
        micState = state
        setMicLook()
    }

    func setSuggestions(_ words: [String]) {
        let shown = Array(words.prefix(cells.count))
        guard shown != self.words else { return }
        self.words = shown
        for (index, cell) in cells.enumerated() {
            let word = index < shown.count ? shown[index] : nil
            cell.setTitle(word, for: .normal)
            cell.accessibilityLabel = word
            cell.isHidden = word == nil
        }
        for (index, divider) in dividers.enumerated() { divider.isHidden = index + 1 >= shown.count }
        setNeedsLayout()
        showBar()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let height = bounds.height
        let size = Self.micSize
        mic.frame = CGRect(x: bounds.width - 4 - size, y: (height - size) / 2, width: size, height: size)
        let content = CGRect(x: 8, y: 0, width: max(mic.frame.minX - 16, 0), height: height)
        status.frame = content
        suggestions.frame = content

        dot.frame = CGRect(x: 0, y: (height - 7) / 2, width: 7, height: 7)
        levels.frame = CGRect(x: 0, y: (height - 16) / 2, width: 22, height: 16)
        let textX = (levels.isHidden ? dot.frame.maxX : levels.frame.maxX) + 8
        statusText.frame = CGRect(x: textX, y: 0, width: max(content.width - textX, 0), height: height)

        let count = max(words.count, 1)
        let width = content.width / CGFloat(count)
        for (index, cell) in cells.enumerated() {
            cell.frame = CGRect(x: CGFloat(index) * width, y: 4, width: width, height: height - 8)
        }
        for (index, divider) in dividers.enumerated() {
            divider.frame = CGRect(x: CGFloat(index + 1) * width, y: (height - 18) / 2, width: 1, height: 18)
        }
    }

    /// One bar beside the mic: suggestions while a word is being typed, the status otherwise.
    /// Recording, sending and Retry keep the status; suggestions wait until they are done.
    private func showBar() {
        let pinned = tone == .recording || tone == .working || tone == .retry
        let showWords = !words.isEmpty && !pinned
        suggestions.isHidden = !showWords
        status.isHidden = showWords
    }

    private func setStatusColours() {
        switch tone {
        case .ready: dot.backgroundColor = palette.success
        case .blocked: dot.backgroundColor = palette.danger
        case .recording: dot.backgroundColor = palette.accent
        case .working: dot.backgroundColor = palette.muted
        case .retry: dot.backgroundColor = palette.warning
        }
    }

    private func setMicLook() {
        let name = micState == .recording ? "stop.fill" : "mic.fill"
        mic.setImage(UIImage(systemName: name, withConfiguration: Self.symbolConfig), for: .normal)
        let disabled = micState == .disabled
        mic.backgroundColor = disabled ? palette.functionKey : palette.accent
        mic.tintColor = disabled ? palette.muted : palette.onAccent
        mic.alpha = micState == .busy ? 0.5 : 1
        mic.accessibilityLabel = micState == .recording ? "Stop dictating" : "Dictate"
    }

    @objc private func micTap() {
        UIDevice.current.playInputClick()
        delegate?.micTapped()
    }

    @objc private func statusTap() {
        delegate?.statusTapped()
    }

    @objc private func suggestionTap(_ sender: SuggestionCell) {
        guard let word = sender.title(for: .normal) else { return }
        UIDevice.current.playInputClick()
        delegate?.suggestionTapped(word)
    }
}

/// A suggestion: plain text on the bar that takes the function-key colour while pressed.
private final class SuggestionCell: UIButton {
    var pressedColour: UIColor = .clear

    override var isHighlighted: Bool {
        didSet { backgroundColor = isHighlighted ? pressedColour : .clear }
    }

    init() {
        super.init(frame: .zero)
        layer.cornerRadius = 6
        layer.cornerCurve = .continuous
        isHidden = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }
}
