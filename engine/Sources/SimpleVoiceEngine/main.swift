// FilePath: engine/Sources/SimpleVoiceEngine/main.swift
// Entry point: `simple-voice-engine --models-dir <path>`, JSON lines on stdin/stdout.

import AppKit
import FluidAudio
import Foundation

/// Routes one request to the feature that implements it. Every request runs in its own task, so
/// a long transcription or download never delays `permissions` or `paste`. A line without an id is
/// a notification: it gets no reply and is applied on the main thread in the order it arrived.
final class Engine: Sendable {
    private let output: Output
    private let store: ModelStore
    private let parakeet: ParakeetEngine
    private let muter = OutputMuter()

    init(modelsDirectory: URL, output: Output) {
        self.output = output
        let store = ModelStore(root: modelsDirectory)
        self.store = store
        parakeet = ParakeetEngine(store: store)
    }

    func accept(_ line: String) {
        let body = Data(line.utf8)
        guard let envelope = try? JSONDecoder().decode(RequestEnvelope.self, from: body) else {
            Log.error("ignoring a line that is not a JSON request: \(line.prefix(200))")
            return
        }
        guard let id = envelope.id else {
            guard let cmd = envelope.cmd else {
                Log.error("ignoring a line without `id` or `cmd`: \(line.prefix(200))")
                return
            }
            // The main queue is FIFO, so notifications keep their order (a hide never overtakes
            // the state it follows); a Task per line would not guarantee that.
            DispatchQueue.main.async {
                MainActor.assumeIsolated { OverlayController.shared.handle(cmd, body: body) }
            }
            return
        }
        let output = output
        guard let cmd = envelope.cmd else {
            Task { await output.failure(id: id, message: "missing `cmd`") }
            return
        }
        let request = Request(id: id, cmd: cmd, body: body)
        Task {
            let progress = ProgressSink(id: id, output: output)
            do {
                let result = try await self.route(request, progress: progress)
                await output.success(id: id, result: result)
            } catch {
                await output.failure(id: id, message: describe(error))
            }
        }
    }

    private func route(_ request: Request, progress: ProgressSink) async throws -> CommandResult {
        switch request.cmd {
        case "ping":
            return VersionResult(version: engineVersion)
        case "model_status":
            let params = try request.params(ModelParams.self)
            let id = try ModelID.parse(params.model)
            return await modelStatus(id, language: params.language)
        case "download_model":
            let params = try request.params(ModelParams.self)
            let id = try ModelID.parse(params.model)
            try await download(id, language: params.language, progress: progress)
            return DownloadResult()
        case "delete_model":
            let id = try ModelID.parse(request.params(ModelParams.self).model)
            try await delete(id)
            return EmptyResult()
        case "preload":
            let params = try request.params(ModelParams.self)
            let id = try ModelID.parse(params.model)
            try await preload(id, language: params.language)
            return EmptyResult()
        case "transcribe":
            let params = try request.params(TranscribeParams.self)
            return try await transcribe(params)
        case "permissions":
            return await Permissions.current()
        case "request_permission":
            let kind = try PermissionKind.parse(request.params(KindParams.self).kind)
            return await Permissions.request(kind)
        case "open_settings":
            let kind = try PermissionKind.parse(request.params(KindParams.self).kind)
            try await MainActor.run { try SystemServices.openSettings(kind) }
            return EmptyResult()
        case "frontmost_app":
            return await MainActor.run { SystemServices.frontmostApp() }
        case "paste":
            try await SystemServices.paste()
            return EmptyResult()
        case "selected_text":
            return try await Selection.read()
        case "prepare_edit_watch":
            try await EditWatcher.shared.prepare()
            return EmptyResult()
        case "watch_edits":
            return try await EditWatcher.shared.watch(request.params(WatchEditsParams.self))
        case "set_output_muted":
            let muted = try request.params(MuteParams.self).muted
            let previous = try await muter.setMuted(muted)
            return MutedResult(previous: previous)
        case "polish":
            let params = try request.params(PolishParams.self)
            return try await polish(params)
        case "watch_fn_key":
            let enabled = try request.params(FnKeyWatchParams.self).enabled
            let output = output
            let active = await MainActor.run { FnKeyMonitor.shared.setEnabled(enabled, output: output) }
            return FnKeyWatchResult(active: active)
        case "login_item":
            return try LoginItem.status()
        case "set_login_item":
            let enabled = try request.params(LoginItemParams.self).enabled
            return try LoginItem.setEnabled(enabled)
        default:
            throw EngineError("unknown command `\(request.cmd)`")
        }
    }

    // MARK: - Models

    private func modelStatus(_ id: ModelID, language: String?) async -> ModelStatusResult {
        switch id {
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash:
            guard let hub = HubModel.forModel(id) else { return .unsupported("unknown model") }
            return store.status(of: hub)
        case .appleSpeech:
            guard #available(macOS 26, *) else { return .unsupported("Apple Speech requires macOS 26 or later") }
            return await AppleSpeech.status(language: language)
        case .appleIntelligence:
            guard #available(macOS 26, *) else {
                return .unsupported("Apple Intelligence requires macOS 26 or later")
            }
            return AppleIntelligence.status()
        }
    }

    private func download(_ id: ModelID, language: String?, progress: ProgressSink) async throws {
        switch id {
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash:
            let hub = try hubModel(id)
            try await store.download(hub, progress: progress)
        case .appleSpeech:
            guard #available(macOS 26, *) else { throw EngineError("Apple Speech requires macOS 26 or later") }
            try await AppleSpeech.download(language: language, progress: progress)
        case .appleIntelligence:
            // macOS installs the Apple Intelligence model itself; there is nothing to fetch.
            let status = await modelStatus(id, language: nil)
            if status.status != .ready {
                throw EngineError(status.reason ?? "Apple Intelligence is unavailable")
            }
        }
    }

    private func delete(_ id: ModelID) async throws {
        switch id {
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash:
            await parakeet.unload(id)
            let hub = try hubModel(id)
            try await store.delete(hub)
        case .appleSpeech:
            guard #available(macOS 26, *) else { throw EngineError("Apple Speech requires macOS 26 or later") }
            await AppleSpeech.releaseReservations()
        case .appleIntelligence:
            throw EngineError("Apple Intelligence is part of macOS and cannot be deleted")
        }
    }

    private func preload(_ id: ModelID, language: String?) async throws {
        switch id {
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash:
            try await parakeet.preload(id)
        case .appleSpeech:
            // SpeechAnalyzer loads per request; preloading verifies the assets are in place.
            guard #available(macOS 26, *) else { throw EngineError("Apple Speech requires macOS 26 or later") }
            try await AppleSpeech.ensureReady(language: language)
        case .appleIntelligence:
            guard #available(macOS 26, *) else {
                throw EngineError("Apple Intelligence requires macOS 26 or later")
            }
            try AppleIntelligence.prewarm()
        }
    }

    private func transcribe(_ params: TranscribeParams) async throws -> TranscriptResult {
        let id = try ModelID.parse(params.model)
        switch id {
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash:
            let samples = try AudioLoader.loadSamples(path: params.wavPath)
            return try await parakeet.transcribe(id, samples: samples, language: params.language)
        case .appleSpeech:
            guard #available(macOS 26, *) else { throw EngineError("Apple Speech requires macOS 26 or later") }
            return try await AppleSpeech.transcribe(path: params.wavPath, language: params.language)
        case .appleIntelligence:
            throw EngineError("apple-intelligence is a post-processing model, not a transcription model")
        }
    }

    private func polish(_ params: PolishParams) async throws -> PolishResult {
        guard #available(macOS 26, *) else {
            throw EngineError("Apple Intelligence requires macOS 26 or later")
        }
        return try await AppleIntelligence.polish(params)
    }

    private func hubModel(_ id: ModelID) throws -> HubModel {
        guard let hub = HubModel.forModel(id) else { throw EngineError("\(id.rawValue) has no downloadable files") }
        return hub
    }
}

// MARK: - Startup

func modelsDirectoryArgument(_ arguments: [String]) -> String? {
    guard let flag = arguments.firstIndex(of: "--models-dir"), arguments.indices.contains(flag + 1) else {
        return nil
    }
    let value = arguments[flag + 1]
    return value.isEmpty ? nil : value
}

if CommandLine.arguments.contains(AccessibilityTrust.checkFlag) {
    AccessibilityTrust.runCheck()
}

guard let modelsPath = modelsDirectoryArgument(CommandLine.arguments) else {
    Log.error("usage: simple-voice-engine --models-dir <path>")
    exit(2)
}

let modelsDirectory = URL(fileURLWithPath: modelsPath, isDirectory: true)
do {
    try FileManager.default.createDirectory(at: modelsDirectory, withIntermediateDirectories: true)
} catch {
    Log.error("cannot create models directory \(modelsPath): \(describe(error))")
    exit(1)
}

// Only protocol lines may reach the app's stdout. The original stdout is kept on a private
// descriptor and fd 1 is pointed at stderr, so a stray print from any framework ends up in the
// log instead of corrupting the JSON stream.
let protocolDescriptor = dup(STDOUT_FILENO)
guard protocolDescriptor >= 0, dup2(STDERR_FILENO, STDOUT_FILENO) >= 0 else {
    Log.error("cannot redirect stdout")
    exit(1)
}

AppLogger.minimumLevel = .info

// The overlay pill is an AppKit window, so the helper is an accessory app: no Dock icon, no menu
// bar, and it never activates.
let application = NSApplication.shared
application.setActivationPolicy(.accessory)

let engine = Engine(
    modelsDirectory: modelsDirectory,
    output: Output(handle: FileHandle(fileDescriptor: protocolDescriptor, closeOnDealloc: false))
)
Log.info("started (version \(engineVersion), models in \(modelsDirectory.path))")

Task.detached { [engine] in
    do {
        for try await line in FileHandle.standardInput.bytes.lines {
            engine.accept(line)
        }
    } catch {
        Log.error("reading stdin failed: \(describe(error))")
    }
    // The app closes stdin when it quits or restarts the helper.
    Log.info("stdin closed, exiting")
    exit(0)
}

// The application's run loop delivers the NSWorkspace notifications behind `frontmost_app`, runs
// the main-actor work (System Settings, frontmost app, the overlay pill) and draws the pill.
application.run()
