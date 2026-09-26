// FilePath: engine/Sources/SimpleVoiceEngine/Overlay.swift
// The floating dictation pill. The app decides what it shows and when (`overlay_*` notifications,
// docs/internal/architecture.md § 6); the helper draws it because only AppKit can put a window over
// full-screen apps (`fullScreenAuxiliary`), and the app's Rust code has no `unsafe` to reach it.
// Being a separate, accessory process also means showing or hiding the pill can never activate
// Simple Voice and pull focus away from the app being dictated into.

import AppKit
import SwiftUI

enum PillPhase: String, Decodable, Sendable {
    case idle
    case recording
    case transcribing
    case formatting
    case done
    case error
    case cancelled
}

/// The `dictation-state` payload the app forwards (sv-domain `DictationState`).
struct PillState: Decodable, Sendable {
    let phase: PillPhase
    let sessionId: UInt64
    /// Unix milliseconds the recording started (recording phase only).
    let startedAt: Int64?
    let message: String?
    let words: Int?
    let note: String?
}

private struct OverlayStateParams: Decodable {
    let state: PillState
}

private struct OverlayVisibleParams: Decodable {
    let visible: Bool
}

private struct OverlayLevelParams: Decodable {
    let level: Double
}

@MainActor
final class OverlayController {
    static let shared = OverlayController()

    /// Points between the pill window and the bottom of the visible frame (above the Dock).
    private static let bottomMargin: CGFloat = 80
    /// How long a finished phase stays on screen before the pill settles back to idle.
    private static let holds: [PillPhase: Duration] = [
        .done: .milliseconds(1100),
        .error: .milliseconds(2000),
        .cancelled: .milliseconds(350),
    ]

    private let model = PillModel()
    private lazy var panel = OverlayPanel(model: model)
    private var hold: Task<Void, Never>?

    private init() {}

    /// Applies one notification. They arrive in the order the app sent them.
    func handle(_ cmd: String, body: Data) {
        let request = Request(id: 0, cmd: cmd, body: body)
        do {
            switch cmd {
            case "overlay_state":
                apply(try request.params(OverlayStateParams.self).state)
            case "overlay_visible":
                setVisible(try request.params(OverlayVisibleParams.self).visible)
            case "overlay_level":
                model.meter.level = min(max(try request.params(OverlayLevelParams.self).level, 0), 1)
            default:
                Log.error("ignoring unknown notification `\(cmd)`")
            }
        } catch {
            Log.error(describe(error))
        }
    }

    private func apply(_ state: PillState) {
        if state.phase == .recording, model.phase != .recording || model.sessionId != state.sessionId {
            model.startedAt = state.startedAt.map { Date(timeIntervalSince1970: Double($0) / 1000) } ?? .now
            model.meter.reset()
        }
        withAnimation(PillStyle.ease) {
            model.phase = state.phase
            model.sessionId = state.sessionId
            model.words = state.words ?? 0
            model.note = state.note
            model.message = state.message
        }
        hold?.cancel()
        hold = nil
        guard let delay = Self.holds[state.phase] else { return }
        hold = Task { [weak self] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled, let self else { return }
            withAnimation(PillStyle.ease) { self.model.phase = .idle }
        }
    }

    private func setVisible(_ visible: Bool) {
        guard visible else {
            panel.orderOut(nil)
            return
        }
        placeUnderCursor()
        panel.orderFrontRegardless()
    }

    /// Bottom-centre of the screen the cursor is on, so the pill appears where the user is looking.
    private func placeUnderCursor() {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
        guard let area = screen?.visibleFrame else { return }
        let origin = NSPoint(
            x: (area.midX - PillStyle.windowSize.width / 2).rounded(),
            y: (area.minY + Self.bottomMargin).rounded()
        )
        panel.setFrameOrigin(origin)
    }
}

/// Borderless, click-through and never key: it floats over every Space, full-screen apps included.
private final class OverlayPanel: NSPanel {
    init(model: PillModel) {
        let frame = NSRect(origin: .zero, size: PillStyle.windowSize)
        super.init(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        // Untitled and buttonless in an accessory app: tiling window managers (AeroSpace) treat it as
        // a popup and leave it alone instead of pinning it to one workspace.
        title = ""
        isFloatingPanel = true
        level = .statusBar
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        backgroundColor = .clear
        isOpaque = false
        hasShadow = false
        ignoresMouseEvents = true
        hidesOnDeactivate = false
        isReleasedWhenClosed = false
        animationBehavior = .none
        let host = NSHostingView(rootView: PillView(model: model))
        host.sizingOptions = []
        host.frame = frame
        contentView = host
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}
