// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceKeyboard/KeyboardViewController.swift
// The Simple Voice keyboard for iPhone: a full keyboard for typing and fixes, and a mic in its top
// bar that has the Simple Voice app record and transcribe, then inserts the text at the cursor.
// The typing rules match the Android keyboard (KeyboardService.kt); the mic flow lives in
// DictationController.

import UIKit

final class KeyboardViewController: UIInputViewController {
    private var root: KeyboardView?
    private var topBar: TopBar?
    private var keys: KeysView?
    private var dictation: DictationController?
    private var heightConstraint: NSLayoutConstraint?
    private lazy var checker = UITextChecker()

    private var layer = Layer.letters
    private var shift = ShiftState.off
    private var lastShiftTap = Date.distantPast
    /// The last key typed a space, so a second one makes a full stop (". ").
    private var lastWasSpace = false
    /// The space before the cursor came from a suggestion or dictation, not the space bar.
    private var autoSpace = false
    /// The letters before the cursor that suggestions would replace.
    private var currentWord = ""

    private static let doubleTap: TimeInterval = 0.3
    private static let wordLookback = 48
    private static let closingPunctuation: Set<String> = [".", ",", "?", "!", ":", ";"]

    override func loadView() {
        let root = KeyboardView(frame: .zero, inputViewStyle: .default)
        self.root = root
        view = root
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        guard let root else { return }
        let palette = Palette.of(traitCollection)
        let topBar = TopBar(palette: palette, delegate: self)
        let keys = KeysView(palette: palette, delegate: self, inputController: self)
        root.embed(topBar: topBar, keys: keys)
        root.apply(palette)
        self.topBar = topBar
        self.keys = keys
        dictation = DictationController(host: self)

        let height = root.heightAnchor.constraint(equalToConstant: root.preferredHeight)
        // Below required so the system's own sizing wins during rotation instead of logging conflicts.
        height.priority = UILayoutPriority(999)
        height.isActive = true
        heightConstraint = height

        registerForTraitChanges([UITraitUserInterfaceStyle.self, UITraitVerticalSizeClass.self]) {
            (controller: KeyboardViewController, _: UITraitCollection) in
            controller.traitsChanged()
        }
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        layer = .letters
        shift = .off
        lastWasSpace = false
        autoSpace = false
        refreshField()
        dictation?.appear()
    }

    override func viewWillDisappear(_ animated: Bool) {
        keys?.finishTouches()
        dictation?.disappear()
        super.viewWillDisappear(animated)
    }

    override func textDidChange(_ textInput: UITextInput?) {
        super.textDidChange(textInput)
        refreshField()
    }

    override func selectionDidChange(_ textInput: UITextInput?) {
        super.selectionDidChange(textInput)
        updateContext()
    }

    /// The field may have changed: its return key, the globe key, capitals and suggestions.
    private func refreshField() {
        keys?.set(returnAction: ReturnAction(textDocumentProxy.returnKeyType))
        keys?.set(showsGlobe: needsInputModeSwitchKey)
        keys?.set(layer: layer, shift: shift)
        updateContext()
    }

    private func traitsChanged() {
        let palette = Palette.of(traitCollection)
        root?.apply(palette)
        topBar?.apply(palette)
        keys?.apply(palette)
        if let root { heightConstraint?.constant = root.preferredHeight }
    }

    // MARK: Typing

    private func handleKey(_ key: Key) {
        let proxy = textDocumentProxy
        let wasSpace = lastWasSpace
        let wasAutoSpace = autoSpace
        lastWasSpace = false
        autoSpace = false
        var textChanged = true
        switch key {
        case .text(let value, _):
            let text = shift == .off ? value : value.uppercased()
            // "word ," → "word, ": punctuation joins the word a suggestion or dictation ended.
            if wasAutoSpace && Self.closingPunctuation.contains(text) && spaceBeforeCursor() {
                proxy.deleteBackward()
                proxy.insertText(text + " ")
                autoSpace = true
            } else {
                proxy.insertText(text)
            }
            if shift == .once { shift = .off }
        case .shift:
            let now = Date()
            if shift == .once && now.timeIntervalSince(lastShiftTap) < Self.doubleTap {
                shift = .locked
            } else {
                shift = shift == .off ? .once : .off
            }
            lastShiftTap = now
            textChanged = false
        case .backspace:
            proxy.deleteBackward()
        case .space:
            typeSpace(afterSpace: wasSpace)
        case .returnKey:
            // A keyboard cannot trigger the field's action directly; the host does on a newline.
            proxy.insertText("\n")
        case .nextKeyboard:
            textChanged = false
        case .switchLayer(let next, _):
            layer = next
            shift = .off
            textChanged = false
            if next == .letters { updateShiftFromCursor() }
        }
        // Symbols are typed in runs; a space after them goes back to the letters, as on iOS.
        if key == .space && layer != .letters { layer = .letters }
        if textChanged { updateContext() }
        keys?.set(layer: layer, shift: shift)
    }

    /// A second space right after a word ends the sentence: "word  " becomes "word. ".
    private func typeSpace(afterSpace: Bool) {
        let proxy = textDocumentProxy
        let before = Array((proxy.documentContextBeforeInput ?? "").suffix(2))
        if afterSpace && before.count == 2 && (before[0].isLetter || before[0].isNumber) && before[1] == " " {
            proxy.deleteBackward()
            proxy.insertText(". ")
            return
        }
        proxy.insertText(" ")
        lastWasSpace = true
    }

    private func spaceBeforeCursor() -> Bool {
        textDocumentProxy.documentContextBeforeInput?.hasSuffix(" ") == true
    }

    private func updateContext() {
        updateShiftFromCursor()
        updateSuggestions()
    }

    /// Capitalises the next letter where the field asks for it (sentence starts, names).
    private func updateShiftFromCursor() {
        guard layer == .letters, shift != .locked else { return }
        shift = wantsCapital() ? .once : .off
        keys?.set(layer: layer, shift: shift)
    }

    private func wantsCapital() -> Bool {
        let proxy = textDocumentProxy
        let before = proxy.documentContextBeforeInput ?? ""
        switch proxy.autocapitalizationType ?? .sentences {
        case .none:
            return false
        case .allCharacters:
            return true
        case .words:
            return before.last.map { $0.isWhitespace } ?? true
        case .sentences:
            guard let last = before.last else { return true }
            if last.isNewline { return true }
            guard last.isWhitespace else { return false }
            guard let end = before.last(where: { !$0.isWhitespace }) else { return true }
            return end == "." || end == "!" || end == "?"
        @unknown default:
            return false
        }
    }

    /// Spelling suggestions from the system checker for the word being typed.
    private func updateSuggestions() {
        let proxy = textDocumentProxy
        let before = String((proxy.documentContextBeforeInput ?? "").suffix(Self.wordLookback))
        currentWord = String(before.reversed().prefix { $0.isLetter || $0 == "'" }.reversed())
        let midWord = proxy.documentContextAfterInput?.first?.isLetter == true
        guard currentWord.count >= 2, !midWord else {
            topBar?.setSuggestions([])
            return
        }
        let range = NSRange(location: 0, length: (currentWord as NSString).length)
        let language = "en_US"
        let misspelled = checker.rangeOfMisspelledWord(
            in: currentWord, range: range, startingAt: 0, wrap: false, language: language
        )
        guard misspelled.location != NSNotFound else {
            topBar?.setSuggestions([])
            return
        }
        let guesses = checker.guesses(forWordRange: range, in: currentWord, language: language) ?? []
        topBar?.setSuggestions(Array(guesses.prefix(3)))
    }
}

extension KeyboardViewController: KeyButtonDelegate {
    func keyTapped(_ key: Key) {
        handleKey(key)
    }

    func keyLongPressed(_ key: Key) {
        switch key {
        case .shift:
            shift = .locked
            keys?.set(layer: layer, shift: shift)
        case .text(_, let alt?):
            handleKey(.text(alt, alt: nil))
        default:
            break
        }
    }

    func spaceDragged(by offset: Int) {
        textDocumentProxy.adjustTextPosition(byCharacterOffset: offset)
        lastWasSpace = false
        autoSpace = false
        updateContext()
    }
}

extension KeyboardViewController: TopBarDelegate {
    func micTapped() {
        dictation?.micTapped()
    }

    func statusTapped() {
        dictation?.statusTapped()
    }

    func suggestionTapped(_ word: String) {
        let proxy = textDocumentProxy
        for _ in currentWord { proxy.deleteBackward() }
        proxy.insertText(word + " ")
        lastWasSpace = false
        updateContext()
        autoSpace = true
    }
}

extension KeyboardViewController: DictationHost {
    func showStatus(_ line: String, tone: Tone) {
        topBar?.setStatus(line, tone: tone)
    }

    func showMic(_ state: MicState) {
        topBar?.setMic(state)
    }

    func showLevel(_ level: Float) {
        topBar?.setLevel(level)
    }

    func insertDictation(_ text: String) {
        // Dictations never run into the word before the cursor (D-16 covers the word after).
        let before = textDocumentProxy.documentContextBeforeInput ?? ""
        let joins = before.last.map { !$0.isWhitespace } ?? false
        textDocumentProxy.insertText(joins ? " " + text : text)
        lastWasSpace = false
        updateContext()
        // Dictated text ends with a space (D-16); a comma typed next joins the last word.
        autoSpace = text.hasSuffix(" ")
    }

    func openApp(_ url: URL, completion: @escaping (Bool) -> Void) {
        guard let context = extensionContext else {
            completion(openThroughApplication(url))
            return
        }
        // NSExtensionContext.open is honoured only for Today widgets; a keyboard gets `false`,
        // so that answer falls through to the application object.
        context.open(url) { [weak self] opened in
            DispatchQueue.main.async {
                completion(opened || self?.openThroughApplication(url) == true)
            }
        }
    }

    /// Keyboards have no API to open their app, but the extension process has a UIApplication at
    /// the end of the responder chain. The old `perform(openURL:)` trick stopped working in
    /// iOS 18 (UIKit makes `openURL:` return NO), so this calls the current
    /// `open(_:options:completionHandler:)` on the object found in the chain.
    private func openThroughApplication(_ url: URL) -> Bool {
        var responder: UIResponder? = self
        while let current = responder {
            if let application = current as? UIApplication {
                application.open(url, options: [:], completionHandler: nil)
                return true
            }
            responder = current.next
        }
        return false
    }
}
