// FilePath: engine/Sources/SimpleVoiceEngine/RunningRequests.swift
// The requests in flight, so `cancel` and shutdown can reach them.

import Foundation

/// Requests in flight, so `cancel` can reach them. Registration happens under the lock that
/// `finished` also takes, so a request that completes at once is never left behind.
final class RunningRequests: @unchecked Sendable {
    private let lock = NSLock()
    private var tasks: [Int64: Task<Void, Never>] = [:]

    func run(_ id: Int64, _ start: () -> Task<Void, Never>) {
        lock.withLock { tasks[id] = start() }
    }

    func finished(_ id: Int64) {
        lock.withLock { _ = tasks.removeValue(forKey: id) }
    }

    func cancel(_ id: Int64) -> Bool {
        guard let task = lock.withLock({ tasks.removeValue(forKey: id) }) else { return false }
        task.cancel()
        return true
    }

    /// Cancels everything in flight and waits, at most `limit`, until each request has answered
    /// (`finished`). A request blocked on the user (an open permission prompt) does not hold up
    /// the exit.
    func cancelAll(waitingAtMost limit: Duration) async {
        let running = lock.withLock { Array(tasks.values) }
        for task in running {
            task.cancel()
        }
        let deadline = ContinuousClock.now + limit
        while !lock.withLock({ tasks.isEmpty }), ContinuousClock.now < deadline {
            try? await Task.sleep(for: .milliseconds(20))
        }
    }
}
