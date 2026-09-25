// FilePath: engine/Sources/SimpleVoiceEngine/AppleSpeech.swift
// On-device Apple Speech (SpeechAnalyzer + SpeechTranscriber, macOS 26+).

import AVFoundation
import Foundation
import Speech

@available(macOS 26, *)
enum AppleSpeech {
    static let unavailableReason = "Apple Speech is not available on this Mac"

    static func status(language: String?) async -> ModelStatusResult {
        guard SpeechTranscriber.isAvailable else { return .unsupported(unavailableReason) }
        guard let locale = await supportedLocale(for: language) else {
            return .unsupported("Apple Speech does not support \(label(requestedLocale(language)))")
        }
        let installed = await SpeechTranscriber.installedLocales
        if installed.contains(where: { $0.identifier(.bcp47) == locale.identifier(.bcp47) }) {
            return .ready()
        }
        return .notDownloaded()
    }

    /// Installs the on-device assets for the locale through AssetInventory, which also reserves
    /// the locale for this app.
    static func download(language: String?, progress: ProgressSink) async throws {
        let locale = try await requireLocale(language)
        let transcriber = SpeechTranscriber(locale: locale, preset: .transcription)
        guard let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) else {
            // Nil means the assets are already installed.
            return
        }
        let message = "Downloading Apple Speech (\(label(locale)))"
        let poller = Task {
            while !Task.isCancelled {
                await progress.report(request.progress.fractionCompleted, message)
                try? await Task.sleep(for: .milliseconds(100))
            }
        }
        defer { poller.cancel() }
        try await request.downloadAndInstall()
        poller.cancel()
        await progress.report(1, "Installed", force: true)
    }

    /// macOS owns the speech assets; releasing the app's reservations lets the system reclaim
    /// the space when nothing else uses them.
    static func releaseReservations() async {
        for locale in await AssetInventory.reservedLocales {
            await AssetInventory.release(reservedLocale: locale)
        }
    }

    /// Fails with a user-facing reason unless the locale's assets are installed.
    static func ensureReady(language: String?) async throws {
        let current = await status(language: language)
        switch current.status {
        case .ready:
            return
        case .notDownloaded:
            throw EngineError(notDownloadedMessage(requestedLocale(language)))
        case .unsupported:
            throw EngineError(current.reason ?? unavailableReason)
        }
    }

    static func transcribe(path: String, language: String?) async throws -> TranscriptResult {
        let locale = try await requireLocale(language)
        let transcriber = SpeechTranscriber(locale: locale, preset: .transcription)
        try await prepareAssets(for: transcriber, locale: locale)
        let file: AVAudioFile
        do {
            file = try AVAudioFile(forReading: URL(fileURLWithPath: path))
        } catch {
            throw EngineError("cannot read audio file \(path): \(describe(error))")
        }

        let analyzer = SpeechAnalyzer(modules: [transcriber])
        // Results must be consumed while the analyzer runs; the stream ends when it finishes.
        let collector = Task {
            var parts: [String] = []
            for try await result in transcriber.results where result.isFinal {
                let text = String(result.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
                if !text.isEmpty {
                    parts.append(text)
                }
            }
            return parts.joined(separator: " ")
        }
        do {
            if let lastSample = try await analyzer.analyzeSequence(from: file) {
                try await analyzer.finalizeAndFinish(through: lastSample)
            } else {
                await analyzer.cancelAndFinishNow()
            }
        } catch {
            collector.cancel()
            await analyzer.cancelAndFinishNow()
            throw error
        }
        let text = try await collector.value
        return TranscriptResult(text: text, language: locale.language.languageCode?.identifier)
    }

    /// `AssetInventory.status` stays below `.installed` until this app reserves the locale, even
    /// when macOS already has its assets. Assets installed system-wide are reserved (and any
    /// remaining installation step run) automatically; only genuinely missing assets fail.
    private static func prepareAssets(for transcriber: SpeechTranscriber, locale: Locale) async throws {
        if await AssetInventory.status(forModules: [transcriber]) == .installed {
            return
        }
        let tag = locale.identifier(.bcp47)
        let installed = await SpeechTranscriber.installedLocales
        guard installed.contains(where: { $0.identifier(.bcp47) == tag }) else {
            throw EngineError(notDownloadedMessage(locale))
        }
        let reserved = await AssetInventory.reservedLocales
        if !reserved.contains(where: { $0.identifier(.bcp47) == tag }) {
            do {
                _ = try await AssetInventory.reserve(locale: locale)
            } catch {
                throw EngineError("cannot reserve Apple Speech for \(label(locale)): \(describe(error))")
            }
        }
        if let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
            try await request.downloadAndInstall()
        }
        guard await AssetInventory.status(forModules: [transcriber]) == .installed else {
            throw EngineError(notDownloadedMessage(locale))
        }
    }

    // MARK: - Locales

    /// `en` → en-US and `hi` → hi-IN (the app's dictation languages); full locales pass through.
    static func requestedLocale(_ language: String?) -> Locale {
        guard let language = language?.trimmingCharacters(in: .whitespacesAndNewlines), !language.isEmpty else {
            return Locale(identifier: "en-US")
        }
        if language.contains("-") || language.contains("_") {
            return Locale(identifier: language)
        }
        switch language.lowercased() {
        case "en":
            return Locale(identifier: "en-US")
        case "hi":
            return Locale(identifier: "hi-IN")
        default:
            return Locale(identifier: language.lowercased())
        }
    }

    /// The exact supported equivalent of the request, else the first supported locale of the
    /// same language (e.g. `en-IN` when only other English regions exist).
    static func supportedLocale(for language: String?) async -> Locale? {
        let requested = requestedLocale(language)
        if let match = await SpeechTranscriber.supportedLocale(equivalentTo: requested) {
            return match
        }
        guard let code = requested.language.languageCode?.identifier else { return nil }
        let supported = await SpeechTranscriber.supportedLocales
        return supported.first { $0.language.languageCode?.identifier == code }
    }

    private static func requireLocale(_ language: String?) async throws -> Locale {
        guard SpeechTranscriber.isAvailable else { throw EngineError(unavailableReason) }
        guard let locale = await supportedLocale(for: language) else {
            throw EngineError("Apple Speech does not support \(label(requestedLocale(language)))")
        }
        return locale
    }

    private static func notDownloadedMessage(_ locale: Locale) -> String {
        "Apple Speech for \(label(locale)) is not downloaded; download it in Settings → Models"
    }

    private static func label(_ locale: Locale) -> String {
        let name = Locale(identifier: "en-US").localizedString(forIdentifier: locale.identifier)
        let tag = locale.identifier(.bcp47)
        return name.map { "\($0) (\(tag))" } ?? tag
    }
}
