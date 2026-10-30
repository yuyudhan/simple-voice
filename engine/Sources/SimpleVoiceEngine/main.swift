// FilePath: engine/Sources/SimpleVoiceEngine/main.swift
// Entry point: `simple-voice-engine --models-dir <path>`, JSON lines on stdin/stdout.

import AppKit
import FluidAudio
import Foundation

/// Routes one request to the feature that implements it. Every request runs in its own task, so
/// a long transcription or download never delays `permissions` or `paste`, and `cancel` can stop
/// it. A line without an id is a notification: it gets no reply. Overlay notifications are
/// applied on the main thread in the order they arrived; `stream_audio` is queued for its live
/// stream right away, also in order.
final class Engine: Sendable {
    private let output: Output
    private let store: ModelStore
    private let parakeet: ParakeetEngine
    private let whisper: WhisperEngine
    private let languageModel: LocalLanguageModel
    private let translator: LocalTranslator
    private let diarizer: SpeakerDiarizer
    private let flashStreaming: FlashStreamingModel
    private let live: LiveStreams
    private let systemAudio: SystemAudioCapture
    private let requests = RunningRequests()
    private let muter = OutputMuter()

    init(modelsDirectory: URL, output: Output) {
        self.output = output
        let store = ModelStore(root: modelsDirectory)
        self.store = store
        parakeet = ParakeetEngine(store: store)
        whisper = WhisperEngine(store: store)
        languageModel = LocalLanguageModel(store: store)
        translator = LocalTranslator(store: store)
        diarizer = SpeakerDiarizer(store: store)
        flashStreaming = FlashStreamingModel(store: store)
        live = LiveStreams(output: output)
        systemAudio = SystemAudioCapture(output: output)
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
            if cmd == "stream_audio" {
                do {
                    live.feed(try Request(id: 0, cmd: cmd, body: body).params(StreamAudioParams.self))
                } catch {
                    Log.error(describe(error))
                }
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
        let requests = requests
        requests.run(id) {
            Task {
                let progress = ProgressSink(id: id, output: output)
                do {
                    let result = try await self.route(request, progress: progress)
                    if Task.isCancelled {
                        await output.failure(id: id, message: Self.cancelled)
                    } else {
                        await output.success(id: id, result: result)
                    }
                } catch {
                    let message = error is CancellationError || Task.isCancelled ? Self.cancelled : describe(error)
                    await output.failure(id: id, message: message)
                }
                requests.finished(id)
            }
        }
    }

    private static let cancelled = "cancelled"

    private func route(_ request: Request, progress: ProgressSink) async throws -> CommandResult {
        switch request.cmd {
        case "ping":
            return VersionResult(version: engineVersion)
        case "cancel":
            let target = try request.params(CancelParams.self).target
            return CancelResult(cancelled: requests.cancel(target))
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
        case "stream_start":
            return try await startStream(request.params(StreamStartParams.self))
        case "stream_finish":
            return try await live.finish(request.params(StreamIdParams.self).streamId)
        case "stream_cancel":
            await live.cancel(try request.params(StreamIdParams.self).streamId)
            return EmptyResult()
        case "diarize":
            return try await diarizer.diarize(request.params(DiarizeParams.self), progress: progress)
        case "system_audio_start":
            return try await systemAudio.start(request.params(SystemAudioStartParams.self))
        case "system_audio_stop":
            return try await systemAudio.stop()
        case "focus_context":
            return await FocusContext.read(try request.params(FocusContextParams.self))
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
            return try await polish(request.params(PolishParams.self), progress: progress)
        case "generate":
            return try await generate(request.params(GenerateParams.self), progress: progress)
        case "translate":
            let params = try request.params(TranslateParams.self)
            guard try ModelID.parse(params.model) == .madlad else {
                throw EngineError("\(params.model) is not a translation model")
            }
            let texts = try await translator.translate(params.texts, into: params.targetLanguage, progress: progress)
            return TranslateResult(texts: texts)
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
        var result = await baseStatus(id, language: language)
        result.streaming = id.streams && result.status != .unsupported
        if id == .qwen3Instruct {
            result.contextTokens = LocalLanguageModel.contextTokens
        }
        return result
    }

    private func baseStatus(_ id: ModelID, language: String?) async -> ModelStatusResult {
        switch id {
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash, .whisperLargeV3Turbo, .whisperHinglish,
            .qwen3Instruct, .speakerDiarization, .madlad, .parakeetVocabulary:
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
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash, .whisperLargeV3Turbo, .whisperHinglish,
            .qwen3Instruct, .speakerDiarization, .madlad, .parakeetVocabulary:
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
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash, .parakeetVocabulary:
            await parakeet.unload(id)
            if id == .parakeetFlash {
                await flashStreaming.unload()
            }
            let hub = try hubModel(id)
            try await store.delete(hub)
        case .whisperLargeV3Turbo, .whisperHinglish:
            await whisper.unload(id)
            let hub = try hubModel(id)
            try await store.delete(hub)
        case .qwen3Instruct:
            await languageModel.unload()
            try await store.delete(try hubModel(id))
        case .speakerDiarization:
            await diarizer.unload()
            try await store.delete(try hubModel(id))
        case .madlad:
            await translator.unload()
            try await store.delete(try hubModel(id))
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
        case .parakeetVocabulary:
            try await parakeet.preloadVocabulary()
        case .whisperLargeV3Turbo, .whisperHinglish:
            try await whisper.preload(id)
        case .qwen3Instruct:
            try await languageModel.preload()
        case .speakerDiarization:
            try await diarizer.preload()
        case .madlad:
            try await translator.preload()
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
        let timestamps = params.timestamps ?? false
        switch id {
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash:
            let samples = try AudioLoader.loadSamples(path: params.wavPath)
            return try await parakeet.transcribe(
                id, samples: samples, language: params.language, timestamps: timestamps,
                vocabulary: params.vocabulary ?? [])
        case .whisperLargeV3Turbo, .whisperHinglish:
            let samples = try AudioLoader.loadSamples(path: params.wavPath)
            let request = WhisperRequest(
                language: params.language,
                languages: params.languages ?? [],
                prompt: params.prompt,
                timestamps: timestamps
            )
            return try await whisper.transcribe(id, samples: samples, request: request)
        case .appleSpeech:
            guard #available(macOS 26, *) else { throw EngineError("Apple Speech requires macOS 26 or later") }
            return try await AppleSpeech.transcribe(
                path: params.wavPath, language: params.language, timestamps: timestamps)
        case .appleIntelligence, .qwen3Instruct, .speakerDiarization, .madlad, .parakeetVocabulary:
            throw EngineError("\(id.rawValue) is not a transcription model")
        }
    }

    // MARK: - Live dictation

    private func startStream(_ params: StreamStartParams) async throws -> StreamStartResult {
        let id = try ModelID.parse(params.model)
        guard id.streams else {
            throw EngineError("\(id.rawValue) does not support live transcription")
        }
        // The previous stream must release shared recogniser state (Parakeet Flash) first.
        await live.cancelAll()
        let recognizer: any LiveRecognizer
        switch id {
        case .parakeetTdtV3, .parakeetTdtV2:
            let (models, version) = try await parakeet.tdtModels(id)
            recognizer = try await SlidingWindowLive.start(models: models, version: version, language: params.language)
        case .parakeetFlash:
            recognizer = try await FlashLive.start(manager: try await flashStreaming.load())
        case .appleSpeech:
            guard #available(macOS 26, *) else { throw EngineError("Apple Speech requires macOS 26 or later") }
            recognizer = try await AppleSpeech.live(language: params.language)
        case .whisperLargeV3Turbo, .whisperHinglish, .appleIntelligence, .qwen3Instruct, .speakerDiarization,
            .madlad, .parakeetVocabulary:
            throw EngineError("\(id.rawValue) does not support live transcription")
        }
        let streamId = await live.start(recognizer, overlay: params.overlay ?? false)
        return StreamStartResult(streamId: streamId)
    }

    // MARK: - Text generation

    private func polish(_ params: PolishParams, progress: ProgressSink) async throws -> PolishResult {
        let id = try ModelID.parse(params.model ?? ModelID.appleIntelligence.rawValue)
        if id == .appleIntelligence {
            guard #available(macOS 26, *) else {
                throw EngineError("Apple Intelligence requires macOS 26 or later")
            }
            return try await AppleIntelligence.polish(params)
        }
        // A rewrite is about as long as its input: roughly a token per three bytes, plus room.
        let budget = min(2_048, params.user.utf8.count / 3 + 256)
        return try await generate(
            id, system: params.system, shots: params.shots, user: params.user, maxTokens: budget, temperature: 0,
            progress: progress)
    }

    private func generate(_ params: GenerateParams, progress: ProgressSink) async throws -> PolishResult {
        if let maxTokens = params.maxTokens, maxTokens < 1 {
            throw EngineError("generate: `maxTokens` must be at least 1")
        }
        return try await generate(
            try ModelID.parse(params.model),
            system: params.system,
            shots: params.shots ?? [],
            user: params.user,
            maxTokens: params.maxTokens ?? 1_024,
            temperature: params.temperature ?? 0.2,
            progress: progress
        )
    }

    private func generate(
        _ id: ModelID,
        system: String,
        shots: [PolishShot],
        user: String,
        maxTokens: Int,
        temperature: Double,
        progress: ProgressSink
    ) async throws -> PolishResult {
        switch id {
        case .appleIntelligence:
            guard #available(macOS 26, *) else {
                throw EngineError("Apple Intelligence requires macOS 26 or later")
            }
            return try await AppleIntelligence.generate(
                system: system, shots: shots, user: user, maxTokens: maxTokens, temperature: temperature)
        case .qwen3Instruct:
            var turns = [ChatTurn(role: "system", content: system)]
            for shot in shots {
                turns.append(ChatTurn(role: "user", content: shot.user))
                turns.append(ChatTurn(role: "assistant", content: shot.assistant))
            }
            turns.append(ChatTurn(role: "user", content: user))
            return try await languageModel.generate(
                turns, maxTokens: maxTokens, temperature: temperature, progress: progress)
        case .parakeetTdtV3, .parakeetTdtV2, .parakeetFlash, .whisperLargeV3Turbo, .whisperHinglish, .appleSpeech,
            .speakerDiarization, .madlad, .parakeetVocabulary:
            throw EngineError("\(id.rawValue) is not a text model")
        }
    }

    private func hubModel(_ id: ModelID) throws -> HubModel {
        guard let hub = HubModel.forModel(id) else { throw EngineError("\(id.rawValue) has no downloadable files") }
        return hub
    }

    /// Stops every request and frees native model memory before the process exits. llama.cpp's
    /// static Metal device aborts in its destructor while a context is still alive.
    func shutdown() async {
        await requests.cancelAll(waitingAtMost: .seconds(5))
        await live.cancelAll()
        await languageModel.unload()
        await translator.unload()
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
    await engine.shutdown()
    exit(0)
}

// The application's run loop delivers the NSWorkspace notifications behind `frontmost_app`, runs
// the main-actor work (System Settings, frontmost app, the overlay pill) and draws the pill.
application.run()
