// FilePath: engine/Sources/SimpleVoiceEngine/AccessibilityTrust.swift
// Accessibility trust that notices a grant made while the helper is running.
//
// `AXIsProcessTrusted()` can keep answering false for the rest of a process's life after the user
// turns Simple Voice on in System Settings, which left the UI warning and the Fn key dead until a
// relaunch. A process started afterwards reads the current grant, so while this one still says no,
// a short-lived copy of the helper run with `--check-accessibility` is asked instead. The copy is
// spawned by this process, so TCC attributes it to Simple Voice.app exactly like its parent.

import ApplicationServices
import Foundation

enum AccessibilityTrust {
    static let checkFlag = "--check-accessibility"

    /// The poller, the Fn key retry and a paste can all ask within the same moment; one child
    /// answers for all of them during this window.
    private static let reuseWindow: Duration = .seconds(1)
    private static let recent = RecentAnswer()

    static func isTrusted() async -> Bool {
        if AXIsProcessTrusted() {
            return true
        }
        let now = ContinuousClock.now
        if let cached = recent.answer(at: now, within: reuseWindow) {
            return cached
        }
        let answer = await askFreshProcess()
        recent.store(answer, at: now)
        return answer
    }

    /// Entry point of the child: exit status 0 means trusted.
    static func runCheck() -> Never {
        exit(AXIsProcessTrusted() ? 0 : 1)
    }

    private static func askFreshProcess() async -> Bool {
        guard let executable = Bundle.main.executableURL else {
            return false
        }
        let process = Process()
        process.executableURL = executable
        process.arguments = [checkFlag]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        return await withCheckedContinuation { (continuation: CheckedContinuation<Bool, Never>) in
            process.terminationHandler = { finished in
                continuation.resume(returning: finished.terminationReason == .exit && finished.terminationStatus == 0)
            }
            do {
                try process.run()
            } catch {
                process.terminationHandler = nil
                Log.error("cannot run the accessibility check: \(describe(error))")
                continuation.resume(returning: false)
            }
        }
    }
}

/// The last child's answer and when it was asked for.
private final class RecentAnswer: @unchecked Sendable {
    private let lock = NSLock()
    private var answer: Bool?
    private var askedAt: ContinuousClock.Instant?

    func answer(at now: ContinuousClock.Instant, within window: Duration) -> Bool? {
        lock.lock()
        defer { lock.unlock() }
        guard let askedAt, now - askedAt < window else {
            return nil
        }
        return answer
    }

    func store(_ answer: Bool, at now: ContinuousClock.Instant) {
        lock.lock()
        defer { lock.unlock() }
        self.answer = answer
        askedAt = now
    }
}
