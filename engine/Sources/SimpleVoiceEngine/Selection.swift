// FilePath: engine/Sources/SimpleVoiceEngine/Selection.swift
// Reads the text selected in the focused app, for edit mode.
//
// Accessibility answers first: native text fields report their selection directly, and a caret
// with nothing selected is a definite "no selection". Apps whose content Accessibility cannot
// see (web pages before Chrome enables its accessibility tree, Electron apps) fall back to a
// synthetic Cmd+C. The user's clipboard is restored right after that copy, so edit mode never
// leaves the selection on it.

import AppKit
import ApplicationServices
import Foundation

enum Selection {
    /// A hung app must not stall the start of an edit; native apps answer within milliseconds.
    private static let messagingTimeout: Float = 0.25
    /// How long the focused app gets to put the copied selection on the pasteboard.
    private static let copyWait: Duration = .milliseconds(400)
    private static let copyPoll: Duration = .milliseconds(15)
    private static let cKey: CGKeyCode = 8

    private enum Focused {
        case text(String)
        case nothingSelected
        case unknown
    }

    static func read() async throws -> SelectedTextResult {
        guard await AccessibilityTrust.isTrusted() else {
            throw EngineError("accessibility permission missing")
        }
        switch focusedSelection() {
        case .text(let text):
            return SelectedTextResult(text: text)
        case .nothingSelected:
            return SelectedTextResult(text: nil)
        case .unknown:
            return SelectedTextResult(text: try await copySelection())
        }
    }

    private static func focusedSelection() -> Focused {
        let system = AXUIElementCreateSystemWide()
        AXUIElementSetMessagingTimeout(system, messagingTimeout)
        guard let focused = attribute(system, kAXFocusedUIElementAttribute),
            CFGetTypeID(focused) == AXUIElementGetTypeID()
        else {
            return .unknown
        }
        let element = unsafeDowncast(focused, to: AXUIElement.self)
        AXUIElementSetMessagingTimeout(element, messagingTimeout)
        if let text = attribute(element, kAXSelectedTextAttribute) as? String,
            !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
            return .text(text)
        }
        // An empty selected text alone is not proof: some apps report "" for content they do not
        // expose. A caret range of length zero is.
        guard let rangeValue = attribute(element, kAXSelectedTextRangeAttribute),
            CFGetTypeID(rangeValue) == AXValueGetTypeID()
        else {
            return .unknown
        }
        var range = CFRange(location: 0, length: 0)
        let value = unsafeDowncast(rangeValue, to: AXValue.self)
        guard AXValueGetValue(value, .cfRange, &range) else {
            return .unknown
        }
        return range.length == 0 ? .nothingSelected : .unknown
    }

    static func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else {
            return nil
        }
        return value
    }

    /// Copies the selection with Cmd+C, reads it, and puts the previous clipboard back. Nothing
    /// arriving on the pasteboard means nothing was selected.
    @MainActor
    private static func copySelection() async throws -> String? {
        let pasteboard = NSPasteboard.general
        let before = pasteboard.changeCount
        let saved = PasteboardSnapshot(pasteboard)
        try await SystemServices.postCommandKey(cKey)
        let deadline = ContinuousClock.now + copyWait
        while pasteboard.changeCount == before, ContinuousClock.now < deadline {
            try await Task.sleep(for: copyPoll)
        }
        guard pasteboard.changeCount != before else {
            return nil
        }
        let copied = pasteboard.string(forType: .string)
        saved.restore(to: pasteboard)
        guard let copied, !copied.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return nil
        }
        return copied
    }
}

/// Every item on the pasteboard with the data of each of its types, so a copy made to read the
/// selection can be undone exactly.
@MainActor
private struct PasteboardSnapshot {
    private let items: [[(NSPasteboard.PasteboardType, Data)]]

    init(_ pasteboard: NSPasteboard) {
        items = (pasteboard.pasteboardItems ?? []).map { item in
            item.types.compactMap { type in item.data(forType: type).map { (type, $0) } }
        }
    }

    func restore(to pasteboard: NSPasteboard) {
        pasteboard.clearContents()
        let restored = items.map { pairs in
            let item = NSPasteboardItem()
            for (type, data) in pairs {
                item.setData(data, forType: type)
            }
            return item
        }
        if !restored.isEmpty {
            pasteboard.writeObjects(restored)
        }
    }
}
