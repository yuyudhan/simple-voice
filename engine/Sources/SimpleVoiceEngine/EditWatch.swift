// FilePath: engine/Sources/SimpleVoiceEngine/EditWatch.swift
// Follows a just-pasted dictation in the focused text field, so the app can learn the words the
// user corrects in it.
//
// The pasted text is located in the field's value once, then the value is re-read on a short
// interval and the span is moved along with every edit, so typing elsewhere in the field never
// reads as a correction. Web views (Chrome, Electron) only build their accessibility tree when
// asked, which is why the app sends `prepare_edit_watch` before it pastes.

import AppKit
import ApplicationServices
import Foundation

/// Owns the single running watch; a newer request ends it with whatever it has read so far.
actor EditWatcher {
    static let shared = EditWatcher()

    /// Helper apps live in `Contents/Frameworks`, or four folders deeper inside Chrome's
    /// versioned framework.
    private static let helperSearchDepth = 5

    private var generation = 0
    private var running: Task<EditWatchResult, Never>?
    /// Processes whose accessibility tree was already switched on; asking again is pointless.
    private var enabledProcesses: Set<pid_t> = []
    private var chromiumBundles: [String: Bool] = [:]

    func prepare() async throws {
        try await Self.requireTrust()
        supersede()
        _ = await enableFrontmostTree()
    }

    func watch(_ params: WatchEditsParams) async throws -> EditWatchResult {
        try await Self.requireTrust()
        let id = supersede()
        let app = await enableFrontmostTree()
        // A newer request may have arrived while the frontmost app was being looked up.
        guard generation == id else {
            return EditWatchResult(text: nil, reason: "superseded")
        }
        let text = params.text
        let window = Duration.milliseconds(max(params.timeoutMs, 0))
        let bundle = app?.bundleId ?? "an unknown app"
        let task = Task.detached {
            await EditTracker.run(pasted: text, window: window, bundle: bundle)
        }
        running = task
        let result = await task.value
        if generation == id {
            running = nil
        }
        return result
    }

    private static func requireTrust() async throws {
        guard await AccessibilityTrust.isTrusted() else {
            throw EngineError("accessibility permission missing")
        }
    }

    /// Cancelling wakes the running watch from its sleep, so it answers right away.
    @discardableResult
    private func supersede() -> Int {
        generation += 1
        running?.cancel()
        running = nil
        return generation
    }

    private func enableFrontmostTree() async -> FrontApp? {
        guard let app = await MainActor.run(body: { FrontApp.current() }) else {
            return nil
        }
        guard enabledProcesses.insert(app.pid).inserted else {
            return app
        }
        let element = AXUIElementCreateApplication(app.pid)
        AXUIElementSetMessagingTimeout(element, EditTracker.messagingTimeout)
        let manual = AXUIElementSetAttributeValue(element, "AXManualAccessibility" as CFString, kCFBooleanTrue)
        // AXEnhancedUserInterface also switches on window animations and layout changes meant for
        // assistive apps, so it is only set on Chromium browsers, which ignore the manual flag.
        if manual != .success, let path = app.bundlePath, isChromium(path) {
            let enhanced = AXUIElementSetAttributeValue(
                element, "AXEnhancedUserInterface" as CFString, kCFBooleanTrue)
            Log.info("asked \(app.bundleId ?? path) for its accessibility tree (AXError \(enhanced.rawValue))")
        }
        return app
    }

    private func isChromium(_ bundlePath: String) -> Bool {
        if let known = chromiumBundles[bundlePath] {
            return known
        }
        let verdict = Self.hasRendererHelper(URL(fileURLWithPath: bundlePath, isDirectory: true))
        chromiumBundles[bundlePath] = verdict
        return verdict
    }

    /// Every Chromium-based app ships a `<Name> Helper (Renderer).app` for its web content.
    private static func hasRendererHelper(_ bundle: URL) -> Bool {
        let frameworks = bundle.appendingPathComponent("Contents/Frameworks", isDirectory: true)
        guard
            let entries = FileManager.default.enumerator(
                at: frameworks, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles])
        else {
            return false
        }
        while let entry = entries.nextObject() as? URL {
            let name = entry.lastPathComponent
            if name.hasSuffix(" Helper (Renderer).app") {
                return true
            }
            if name.hasSuffix(".app") || entries.level >= helperSearchDepth {
                entries.skipDescendants()
            }
        }
        return false
    }
}

/// The frontmost app as read on the main thread, where NSWorkspace keeps it current.
private struct FrontApp: Sendable {
    let pid: pid_t
    let bundleId: String?
    let bundlePath: String?

    @MainActor
    static func current() -> FrontApp? {
        guard let app = NSWorkspace.shared.frontmostApplication else {
            return nil
        }
        return FrontApp(pid: app.processIdentifier, bundleId: app.bundleIdentifier, bundlePath: app.bundleURL?.path)
    }
}

/// The Accessibility side of one watch. It runs on its own task, never the main thread, and
/// keeps every AXUIElement inside that task.
private enum EditTracker {
    /// A hung app must not stall the watch; native apps answer within milliseconds.
    static let messagingTimeout: Float = 0.25
    /// A web view can take a moment to build its tree and apply the paste after being asked.
    private static let locateFor: Duration = .milliseconds(1500)
    private static let locatePoll: Duration = .milliseconds(50)
    private static let trackPoll: Duration = .milliseconds(150)
    private static let unreadable = "the focused field does not expose its text"
    private static let notFound = "the pasted text was not found in the field"

    private enum Located {
        case found(AXUIElement, FieldSpan)
        case missing(String)
    }

    static func run(pasted: String, window: Duration, bundle: String) async -> EditWatchResult {
        let deadline = ContinuousClock.now + window
        Log.info("edit watch started in \(bundle)")
        switch await locate(pasted, until: min(deadline, ContinuousClock.now + locateFor)) {
        case .found(let element, let span):
            let (text, reason) = await track(element, span, until: deadline)
            Log.info("edit watch in \(bundle) ended: \(reason)")
            return EditWatchResult(text: text, reason: nil)
        case .missing(let reason):
            Log.info("edit watch in \(bundle) ended: \(reason)")
            return EditWatchResult(text: nil, reason: reason)
        }
    }

    private static func locate(_ pasted: String, until deadline: ContinuousClock.Instant) async -> Located {
        let needle = pasted.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !needle.isEmpty else {
            return .missing(notFound)
        }
        var reason = unreadable
        repeat {
            if let element = focusedElement() {
                if attribute(element, kAXSubroleAttribute) as? String == kAXSecureTextFieldSubrole {
                    return .missing("secure field")
                }
                if let value = value(of: element) {
                    if let span = FieldSpan(value: value, needle: needle, caret: caret(of: element)) {
                        return .found(element, span)
                    }
                    reason = notFound
                } else {
                    reason = unreadable
                }
            } else {
                reason = unreadable
            }
            try? await Task.sleep(for: locatePoll)
        } while !Task.isCancelled && ContinuousClock.now < deadline
        return .missing(reason)
    }

    /// Returns the last non-empty reading of the span and why the watch ended.
    private static func track(
        _ element: AXUIElement,
        _ located: FieldSpan,
        until deadline: ContinuousClock.Instant
    ) async -> (text: String, reason: String) {
        var span = located
        var snapshot = span.text
        while true {
            try? await Task.sleep(for: trackPoll)
            if Task.isCancelled {
                return (snapshot, "superseded")
            }
            guard let focused = focusedElement(), CFEqual(focused, element) else {
                return (snapshot, "focus moved")
            }
            if let value = value(of: element) {
                // Chat apps clear the field when the message is sent; the text before that counts.
                if value.isEmpty {
                    return (snapshot, "field cleared")
                }
                span.update(to: value)
                if span.isEmpty {
                    return (snapshot, "field cleared")
                }
                snapshot = span.text
            }
            if ContinuousClock.now >= deadline {
                return (snapshot, "window closed")
            }
        }
    }

    private static func focusedElement() -> AXUIElement? {
        let system = AXUIElementCreateSystemWide()
        AXUIElementSetMessagingTimeout(system, messagingTimeout)
        guard let focused = attribute(system, kAXFocusedUIElementAttribute),
            CFGetTypeID(focused) == AXUIElementGetTypeID()
        else {
            return nil
        }
        let element = unsafeDowncast(focused, to: AXUIElement.self)
        AXUIElementSetMessagingTimeout(element, messagingTimeout)
        return element
    }

    private static func value(of element: AXUIElement) -> String? {
        attribute(element, kAXValueAttribute) as? String
    }

    private static func caret(of element: AXUIElement) -> Int? {
        guard let rangeValue = attribute(element, kAXSelectedTextRangeAttribute),
            CFGetTypeID(rangeValue) == AXValueGetTypeID()
        else {
            return nil
        }
        var range = CFRange(location: 0, length: 0)
        guard AXValueGetValue(unsafeDowncast(rangeValue, to: AXValue.self), .cfRange, &range) else {
            return nil
        }
        return range.location
    }

    private static func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
        Selection.attribute(element, name)
    }
}
