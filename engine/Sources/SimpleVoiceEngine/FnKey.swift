// FilePath: engine/Sources/SimpleVoiceEngine/FnKey.swift
// Watches the Fn (Globe) key for the dictation shortcuts. The app's global shortcuts are Carbon
// hot keys, which cannot register a lone modifier, so a listen-only event tap reports Fn here as
// unsolicited `fn_key` events instead (docs/internal/architecture.md § 6).

import ApplicationServices
import Foundation

struct FnKeyWatchParams: Decodable {
    let enabled: Bool
}

struct FnKeyWatchResult: Encodable, Sendable {
    let active: Bool
}

enum FnKeyAction: String, Encodable, Sendable {
    /// Held for `holdDelay` with nothing else pressed.
    case press
    /// Released after `press`.
    case release
    /// Released before `holdDelay`, with nothing else pressed.
    case tap
    /// Another key or modifier joined after `press`: Fn was a modifier (Fn+Arrow) after all.
    case chord
}

@MainActor
final class FnKeyMonitor {
    static let shared = FnKeyMonitor()

    /// Fn+Arrow and Fn+Delete usually follow the Fn press within this window, so a press is only
    /// reported once it outlasts it; that keeps those combinations from starting a dictation.
    private static let holdDelay: TimeInterval = 0.1
    /// Event taps need Accessibility access, which may be granted while the app runs.
    private static let retryInterval: TimeInterval = 2
    /// `kVK_Function`.
    private static let fnKeyCode: Int64 = 0x3F
    private static let otherModifiers: CGEventFlags = [
        .maskShift, .maskControl, .maskAlternate, .maskCommand,
    ]

    private let actions: AsyncStream<FnKeyAction>.Continuation
    private let stream: AsyncStream<FnKeyAction>
    private var output: Output?
    private var enabled = false
    private var tap: CFMachPort?
    private var source: CFRunLoopSource?
    private var retry: Timer?
    private var holdTimer: Timer?
    private var fnDown = false
    private var pressed = false
    /// Set once Fn turns out to be part of a combination; the rest of that press is ignored.
    private var chorded = false

    private init() {
        (stream, actions) = AsyncStream.makeStream(of: FnKeyAction.self)
    }

    /// Returns whether the tap is installed. Without Accessibility access it keeps retrying.
    func setEnabled(_ enabled: Bool, output: Output) -> Bool {
        if self.output == nil {
            self.output = output
            // One consumer keeps the actions in the order they happened.
            let stream = stream
            Task.detached {
                for await action in stream {
                    await output.fnKey(action)
                }
            }
        }
        self.enabled = enabled
        if enabled {
            install()
        } else {
            uninstall()
        }
        return tap != nil
    }

    private func install() {
        guard tap == nil else { return }
        // Without the permission macOS still creates the tap but delivers no key events, so the
        // permission is checked up front. It is the Accessibility grant paste needs as well, and
        // the one the UI warns about while Fn is configured.
        guard AXIsProcessTrusted() else {
            scheduleRetry()
            return
        }
        let mask = (1 << CGEventType.flagsChanged.rawValue) | (1 << CGEventType.keyDown.rawValue)
        guard
            let created = CGEvent.tapCreate(
                tap: .cgSessionEventTap,
                place: .headInsertEventTap,
                options: .listenOnly,
                eventsOfInterest: CGEventMask(mask),
                callback: fnKeyTapCallback,
                userInfo: nil
            )
        else {
            scheduleRetry()
            return
        }
        let runLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, created, 0)
        CFRunLoopAddSource(CFRunLoopGetMain(), runLoopSource, .commonModes)
        CGEvent.tapEnable(tap: created, enable: true)
        tap = created
        source = runLoopSource
        retry?.invalidate()
        retry = nil
        Log.info("watching the Fn key")
    }

    private func scheduleRetry() {
        guard retry == nil else { return }
        Log.info("cannot watch the Fn key without Accessibility access; retrying")
        retry = Timer.scheduledTimer(withTimeInterval: Self.retryInterval, repeats: true) { _ in
            MainActor.assumeIsolated {
                let monitor = FnKeyMonitor.shared
                if monitor.enabled {
                    monitor.install()
                } else {
                    monitor.uninstall()
                }
            }
        }
    }

    private func uninstall() {
        retry?.invalidate()
        retry = nil
        cancelHold()
        fnDown = false
        pressed = false
        chorded = false
        if let source {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), source, .commonModes)
        }
        if let tap {
            CGEvent.tapEnable(tap: tap, enable: false)
            CFMachPortInvalidate(tap)
        }
        tap = nil
        source = nil
    }

    fileprivate func handle(type: CGEventType, keyCode: Int64, flags: CGEventFlags) {
        switch type {
        case .tapDisabledByTimeout, .tapDisabledByUserInput:
            if let tap { CGEvent.tapEnable(tap: tap, enable: true) }
        case .flagsChanged where keyCode == Self.fnKeyCode:
            fnChanged(down: flags.contains(.maskSecondaryFn), flags: flags)
        case .flagsChanged, .keyDown:
            if fnDown { joined() }
        default:
            break
        }
    }

    private func fnChanged(down: Bool, flags: CGEventFlags) {
        if down {
            guard !fnDown else { return }
            fnDown = true
            pressed = false
            // Fn added to a combination that is already held is not a press of Fn alone.
            chorded = !flags.intersection(Self.otherModifiers).isEmpty
            if !chorded { startHold() }
            return
        }
        guard fnDown else { return }
        fnDown = false
        cancelHold()
        if !chorded {
            actions.yield(pressed ? .release : .tap)
        }
        pressed = false
        chorded = false
    }

    private func joined() {
        guard !chorded else { return }
        chorded = true
        cancelHold()
        if pressed { actions.yield(.chord) }
    }

    private func startHold() {
        holdTimer = Timer.scheduledTimer(withTimeInterval: Self.holdDelay, repeats: false) { _ in
            MainActor.assumeIsolated {
                FnKeyMonitor.shared.holdElapsed()
            }
        }
    }

    private func holdElapsed() {
        holdTimer = nil
        guard fnDown, !chorded, !pressed else { return }
        pressed = true
        actions.yield(.press)
    }

    private func cancelHold() {
        holdTimer?.invalidate()
        holdTimer = nil
    }
}

/// The tap's run loop source is on the main run loop, so the callback runs on the main thread.
private func fnKeyTapCallback(
    proxy _: CGEventTapProxy,
    type: CGEventType,
    event: CGEvent,
    userInfo _: UnsafeMutableRawPointer?
) -> Unmanaged<CGEvent>? {
    let keyCode = event.getIntegerValueField(.keyboardEventKeycode)
    let flags = event.flags
    MainActor.assumeIsolated {
        FnKeyMonitor.shared.handle(type: type, keyCode: keyCode, flags: flags)
    }
    return Unmanaged.passUnretained(event)
}
