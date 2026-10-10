// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceKeyboard/KeyLayout.swift
// The keys and rows of each layer, the same layout as the Android keyboard's KeyLayout.kt: a phone
// QWERTY with `123` and `#+=` layers. The top letter row holds the digits on a long press, and
// comma and full stop sit beside the space bar so a sentence never needs a layer switch.

import UIKit

/// One key on the keyboard. `text` types its text; the others act.
enum Key: Equatable {
    /// Types `text`; a long press types `alt` instead, shown small in the key's corner.
    case text(String, alt: String?)
    case shift
    case backspace
    case space
    case returnKey
    case nextKeyboard
    case switchLayer(Layer, label: String)

    var isFunction: Bool {
        switch self {
        case .text, .space: return false
        case .shift, .backspace, .returnKey, .nextKeyboard, .switchLayer: return true
        }
    }

    /// Share of the row's width, in letter-key units.
    var weight: CGFloat {
        switch self {
        case .text: return 1
        case .space: return 4
        case .returnKey: return 1.8
        case .shift, .backspace: return 1.5
        case .nextKeyboard: return 1.1
        case .switchLayer: return 1.4
        }
    }
}

enum Layer {
    case letters, numbers, symbols
}

/// Off, the next letter only, or every letter until tapped again (a double tap).
enum ShiftState {
    case off, once, locked
}

/// What the return key does in this field, and the SF Symbol that says so.
enum ReturnAction {
    case newline, send, search, go, next, done

    init(_ type: UIReturnKeyType?) {
        switch type {
        case .send: self = .send
        case .search, .google, .yahoo: self = .search
        case .go, .route, .join: self = .go
        case .next, .continue: self = .next
        case .done: self = .done
        default: self = .newline
        }
    }

    var symbol: String {
        switch self {
        case .newline: return "return"
        case .send: return "paperplane"
        case .search: return "magnifyingglass"
        case .go, .next: return "arrow.right"
        case .done: return "checkmark"
        }
    }

    var label: String {
        switch self {
        case .newline: return "Return"
        case .send: return "Send"
        case .search: return "Search"
        case .go: return "Go"
        case .next: return "Next"
        case .done: return "Done"
        }
    }
}

enum KeyLayout {
    static func rows(_ layer: Layer) -> [[Key]] {
        switch layer {
        case .letters:
            let top = "qwertyuiop".enumerated().map { index, letter in
                Key.text(String(letter), alt: String((index + 1) % 10))
            }
            return [
                top,
                letters("asdfghjkl"),
                [.shift] + letters("zxcvbnm") + [.backspace],
                [
                    .switchLayer(.numbers, label: "123"),
                    .nextKeyboard,
                    .text(",", alt: "!"),
                    .space,
                    .text(".", alt: "?"),
                    .returnKey,
                ],
            ]
        case .numbers:
            return [
                letters("1234567890"),
                texts("-", "/", ":", ";", "(", ")", "$", "&", "@", "\""),
                [.switchLayer(.symbols, label: "#+=")] + texts(".", ",", "?", "!", "'") + [.backspace],
                bottom(.switchLayer(.letters, label: "ABC")),
            ]
        case .symbols:
            return [
                texts("[", "]", "{", "}", "#", "%", "^", "*", "+", "="),
                texts("_", "\\", "|", "~", "<", ">", "€", "£", "¥", "•"),
                [.switchLayer(.numbers, label: "123")] + texts(".", ",", "?", "!", "'") + [.backspace],
                bottom(.switchLayer(.letters, label: "ABC")),
            ]
        }
    }

    private static func bottom(_ layerKey: Key) -> [Key] {
        [layerKey, .nextKeyboard, .space, .returnKey]
    }

    private static func letters(_ characters: String) -> [Key] {
        characters.map { Key.text(String($0), alt: nil) }
    }

    private static func texts(_ values: String...) -> [Key] {
        values.map { Key.text($0, alt: nil) }
    }
}
