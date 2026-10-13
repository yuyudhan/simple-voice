// FilePath: engine/Sources/SimpleVoiceEngine/FocusContext.swift
// Optional dictation context: a bounded piece of the focused text field around the caret, the
// window title and the app, read through Accessibility only when the app asks (`focus_context`).
// Excluded apps are never read, secure fields never yield text, and the text is never logged.

import AppKit
import ApplicationServices
import Foundation

struct FocusContextParams: Decodable {
    let maxChars: Int
    let excludedBundleIds: [String]?
}

struct FocusContextResult: Encodable, Sendable {
    var appName: String? = nil
    var bundleId: String? = nil
    var windowTitle: String? = nil
    var role: String? = nil
    var text: String? = nil
    /// Caret position inside `text`, counted in Unicode scalars (Rust `char`s).
    var caret: Int? = nil
    /// Why there is no text: `permission_denied`, `excluded_app`, `secure_field`, `no_focused_text`.
    var reason: String? = nil
}

enum FocusContext {
    static let maximumChars = 4_000
    private static let messagingTimeout: Float = 0.25

    static func read(_ params: FocusContextParams) async -> FocusContextResult {
        guard let app = await MainActor.run(body: { FrontApp.current() }) else {
            return FocusContextResult(reason: "no_focused_text")
        }
        var result = FocusContextResult(appName: app.name, bundleId: app.bundleId)
        if let bundle = app.bundleId, (params.excludedBundleIds ?? []).contains(bundle) {
            result.reason = "excluded_app"
            return result
        }
        guard await AccessibilityTrust.isTrusted() else {
            result.reason = "permission_denied"
            return result
        }
        await EditWatcher.shared.enableTree(of: app)
        let limit = min(max(params.maxChars, 0), maximumChars)
        let pid = app.pid
        let read = await Task.detached { readFocused(pid: pid, limit: limit) }.value
        result.windowTitle = read.windowTitle
        result.role = read.role
        result.text = read.text
        result.caret = read.caret
        result.reason = read.reason
        Log.info("focus context from \(app.bundleId ?? "an unknown app"): \(read.reason ?? "text")")
        return result
    }

    private struct Read: Sendable {
        var windowTitle: String?
        var role: String?
        var text: String?
        var caret: Int?
        var reason: String?
    }

    private static func readFocused(pid: pid_t, limit: Int) -> Read {
        let application = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(application, messagingTimeout)
        var read = Read()
        if let window = element(attribute(application, kAXFocusedWindowAttribute)) {
            AXUIElementSetMessagingTimeout(window, messagingTimeout)
            read.windowTitle = nonEmpty(attribute(window, kAXTitleAttribute) as? String)
        }
        guard let focused = element(attribute(application, kAXFocusedUIElementAttribute)) else {
            read.reason = "no_focused_text"
            return read
        }
        AXUIElementSetMessagingTimeout(focused, messagingTimeout)
        let role = attribute(focused, kAXRoleAttribute) as? String
        let subrole = attribute(focused, kAXSubroleAttribute) as? String
        if role == "AXSecureTextField" || subrole == kAXSecureTextFieldSubrole {
            return Read(role: role, reason: "secure_field")
        }
        read.role = role
        guard limit > 0, let found = readText(of: focused, limit: limit),
            !found.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        else {
            read.reason = "no_focused_text"
            return read
        }
        // Focus moved while reading: the text may belong to a field the user already left.
        guard let after = element(attribute(application, kAXFocusedUIElementAttribute)), CFEqual(after, focused) else {
            read.reason = "no_focused_text"
            return read
        }
        read.text = found.text
        read.caret = found.caret
        return read
    }

    /// At most `limit` UTF-16 units around the caret (two thirds before it), read with
    /// kAXStringForRange so a long document is never copied whole; fields without that
    /// attribute fall back to their value.
    private static func readText(of element: AXUIElement, limit: Int) -> (text: String, caret: Int?)? {
        let selection = selectedRange(of: element)
        if let count = (attribute(element, kAXNumberOfCharactersAttribute) as? NSNumber)?.intValue, count >= 0 {
            let range = readWindow(around: selection?.location, count: count, limit: limit)
            if range.length == 0 {
                return nil
            }
            if let text = string(of: element, in: range) {
                return (text, caretOffset(selection?.location, in: range, text: text))
            }
        }
        guard let value = attribute(element, kAXValueAttribute) as? String else { return nil }
        let full = value as NSString
        let range = readWindow(around: selection?.location, count: full.length, limit: limit)
        guard range.length > 0 else { return nil }
        let safe = full.rangeOfComposedCharacterSequences(for: NSRange(location: range.location, length: range.length))
        let text = full.substring(with: safe)
        return (
            text,
            caretOffset(selection?.location, in: CFRange(location: safe.location, length: safe.length), text: text)
        )
    }

    /// The range to read: centred two-thirds behind the caret, else the end of the field (where
    /// a chat or commit message is being written).
    private static func readWindow(around caret: Int?, count: Int, limit: Int) -> CFRange {
        guard count > 0 else { return CFRange(location: 0, length: 0) }
        let anchor = min(max(caret ?? count, 0), count)
        let before = caret == nil ? limit : limit * 2 / 3
        var start = max(anchor - before, 0)
        let end = min(start + limit, count)
        start = max(end - limit, 0)
        return CFRange(location: start, length: end - start)
    }

    private static func caretOffset(_ location: Int?, in window: CFRange, text: String) -> Int? {
        guard let location, location >= window.location, location <= window.location + window.length else {
            return nil
        }
        let utf16 = text.utf16
        let offset = min(location - window.location, utf16.count)
        guard let index = utf16.index(utf16.startIndex, offsetBy: offset, limitedBy: utf16.endIndex),
            let scalarIndex = index.samePosition(in: text.unicodeScalars)
        else {
            return nil
        }
        return text.unicodeScalars.distance(from: text.unicodeScalars.startIndex, to: scalarIndex)
    }

    private static func selectedRange(of element: AXUIElement) -> CFRange? {
        guard let value = attribute(element, kAXSelectedTextRangeAttribute),
            CFGetTypeID(value) == AXValueGetTypeID()
        else {
            return nil
        }
        var range = CFRange(location: 0, length: 0)
        guard AXValueGetValue(unsafeDowncast(value, to: AXValue.self), .cfRange, &range) else { return nil }
        return range
    }

    private static func string(of element: AXUIElement, in range: CFRange) -> String? {
        var mutable = range
        guard let parameter = AXValueCreate(.cfRange, &mutable) else { return nil }
        var value: CFTypeRef?
        let status = AXUIElementCopyParameterizedAttributeValue(
            element, kAXStringForRangeParameterizedAttribute as CFString, parameter, &value)
        guard status == .success else { return nil }
        return value as? String
    }

    private static func element(_ value: CFTypeRef?) -> AXUIElement? {
        guard let value, CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
        return unsafeDowncast(value, to: AXUIElement.self)
    }

    private static func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
        Selection.attribute(element, name)
    }

    private static func nonEmpty(_ text: String?) -> String? {
        guard let text, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        return text
    }
}
