// FilePath: engine/Sources/SimpleVoiceEngine/ResidentModel.swift
// The lifecycle shared by the downloaded llama.cpp models (the Qwen text model and the MADLAD
// translator): load on first use, one call at a time, unload after a quiet spell or on exit.

import Foundation
import llama

/// llama.cpp's process-wide backend, initialised once before the first model loads.
enum LlamaBackend {
    private static let initialised: Void = {
        llama_backend_init()
        // llama.cpp logs model metadata and timings by default; only its errors go to the log.
        llama_log_set(
            { level, text, _ in
                guard level == GGML_LOG_LEVEL_ERROR, let text else { return }
                Log.error("llama.cpp: \(String(cString: text).trimmingCharacters(in: .whitespacesAndNewlines))")
            }, nil)
    }()

    static func ensure() {
        _ = initialised
    }
}

/// One downloaded model file kept in memory between requests. Calls into the runtime run one at
/// a time through the gate. The runtime frees its native memory when released, which `unload`
/// does after `idleUnload` without requests and on exit (llama.cpp's static Metal device asserts
/// in its destructor if a context is still alive).
actor ResidentModel<Runtime: Sendable> {
    private let id: ModelID
    private let file: String
    private let store: ModelStore
    private let idleUnload: Duration
    private let load: @Sendable (String) throws -> Runtime
    private let gate = SerialGate()
    private var runtime: Runtime?
    private var loading: Task<Runtime, any Error>?
    private var lastUse = ContinuousClock.now
    private var idleTimer: Task<Void, Never>?

    init(
        id: ModelID,
        file: String,
        store: ModelStore,
        idleUnload: Duration = .seconds(600),
        load: @escaping @Sendable (String) throws -> Runtime
    ) {
        self.id = id
        self.file = file
        self.store = store
        self.idleUnload = idleUnload
        self.load = load
    }

    func preload() async throws {
        _ = try await loaded()
        scheduleUnload()
    }

    func unload() async {
        idleTimer?.cancel()
        idleTimer = nil
        if let loading {
            _ = try? await loading.value
        }
        guard runtime != nil else { return }
        // Waits for a running call; the runtime frees its memory when released.
        _ = try? await gate.run { () -> Bool in true }
        runtime = nil
        Log.info("unloaded \(id.rawValue)")
    }

    /// Runs `body` on the loaded runtime, after any call already running.
    func run<T: Sendable>(_ body: @escaping @Sendable (Runtime) async throws -> T) async throws -> T {
        let runtime = try await loaded()
        defer { scheduleUnload() }
        return try await gate.run { try await body(runtime) }
    }

    /// The loaded runtime. A load is not interruptible (llama.cpp reads gigabytes), but a
    /// cancelled caller stops waiting at once; the load keeps going and is kept for the next call.
    private func loaded() async throws -> Runtime {
        lastUse = .now
        if let runtime { return runtime }
        if let loading { return try await waitCancellably(loading) }
        guard let hub = HubModel.forModel(id) else {
            throw EngineError("\(id.rawValue) has no downloadable files")
        }
        let path = try store.readyDirectory(for: hub).appendingPathComponent(file).path
        let load = load
        let task = Task { () throws -> Runtime in
            defer { loading = nil }
            let loaded = try await Task.detached { try load(path) }.value
            runtime = loaded
            return loaded
        }
        loading = task
        return try await waitCancellably(task)
    }

    private func scheduleUnload() {
        lastUse = .now
        idleTimer?.cancel()
        let delay = idleUnload
        idleTimer = Task { [weak self] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled, let self else { return }
            await self.unloadIfIdle()
        }
    }

    private func unloadIfIdle() async {
        guard ContinuousClock.now - lastUse >= idleUnload else { return }
        await unload()
    }
}

/// Waits for `task` but gives up with `CancellationError` as soon as the caller is cancelled,
/// leaving `task` running.
func waitCancellably<T: Sendable>(_ task: Task<T, any Error>) async throws -> T {
    let once = ResumeOnce<T>()
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<T, any Error>) in
            once.attach(continuation)
            Task {
                do {
                    once.resume(.success(try await task.value))
                } catch {
                    once.resume(.failure(error))
                }
            }
        }
    } onCancel: {
        once.resume(.failure(CancellationError()))
    }
}

/// Resumes a continuation exactly once, whichever of the outcome or a cancellation comes first,
/// even when the cancellation arrives before the continuation exists.
private final class ResumeOnce<T: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<T, any Error>?
    private var outcome: Result<T, any Error>?
    private var done = false

    func attach(_ continuation: CheckedContinuation<T, any Error>) {
        let early = lock.withLock { () -> Result<T, any Error>? in
            if let outcome {
                done = true
                return outcome
            }
            self.continuation = continuation
            return nil
        }
        if let early {
            continuation.resume(with: early)
        }
    }

    func resume(_ result: Result<T, any Error>) {
        let waiting = lock.withLock { () -> CheckedContinuation<T, any Error>? in
            guard !done, outcome == nil else { return nil }
            guard let continuation else {
                outcome = result
                return nil
            }
            done = true
            self.continuation = nil
            return continuation
        }
        waiting?.resume(with: result)
    }
}
