// FilePath: mobile/plugins/sv-native/ios/Sources/SvNative/AppleSpeech.swift
// On-device transcription for dictating offline (MB-4), with SpeechAnalyzer (iOS 26 or later).
// Its model is downloaded while online so it is there when the connection is not.

import AVFoundation
import Foundation
import Speech

struct Heard {
    let text: String
    let language: String?
    let transcribeMs: Int64
}

final class AppleSpeech {
    /// Whether a recording can be transcribed on this phone now. Updated by `refresh`.
    private(set) var isAvailable = false

    /// Checks the model for the phone's language; while online, downloads it if missing.
    func refresh(online: Bool, done: @escaping () -> Void) {
        guard #available(iOS 26, *) else {
            done()
            return
        }
        Task {
            let ready = await Self.prepare(download: online)
            await MainActor.run {
                self.isAvailable = ready
                done()
            }
        }
    }

    /// Transcribes a finished WAV recording. Called on the main thread; `done` runs there too.
    func transcribe(_ url: URL, done: @escaping (Result<Heard, Error>) -> Void) {
        guard #available(iOS 26, *) else {
            done(.failure(SpeechError.unsupported))
            return
        }
        Task {
            let result: Result<Heard, Error>
            do {
                result = .success(try await Self.run(url))
            } catch {
                result = .failure(error)
            }
            await MainActor.run { done(result) }
        }
    }

    @available(iOS 26, *)
    private static func transcriber() async -> SpeechTranscriber? {
        guard SpeechTranscriber.isAvailable,
              let locale = await SpeechTranscriber.supportedLocale(equivalentTo: Locale.current)
        else { return nil }
        return SpeechTranscriber(locale: locale, preset: .transcription)
    }

    @available(iOS 26, *)
    private static func prepare(download: Bool) async -> Bool {
        guard let transcriber = await transcriber() else { return false }
        if await AssetInventory.status(forModules: [transcriber]) == .installed { return true }
        guard download else { return false }
        do {
            try await AssetInventory.assetInstallationRequest(supporting: [transcriber])?.downloadAndInstall()
        } catch {
            return false
        }
        return await AssetInventory.status(forModules: [transcriber]) == .installed
    }

    @available(iOS 26, *)
    private static func run(_ url: URL) async throws -> Heard {
        guard let transcriber = await transcriber() else { throw SpeechError.unsupported }
        let started = Date()
        let file = try AVAudioFile(forReading: url)
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        async let text = transcriber.results.reduce(into: "") { text, result in
            if result.isFinal { text += String(result.text.characters) }
        }
        if let end = try await analyzer.analyzeSequence(from: file) {
            try await analyzer.finalizeAndFinish(through: end)
        } else {
            await analyzer.cancelAndFinishNow()
        }
        let heard = try await text
        let language = transcriber.selectedLocales.first?.language.languageCode?.identifier
        let elapsed = Int64(Date().timeIntervalSince(started) * 1000)
        return Heard(text: heard.trimmingCharacters(in: .whitespacesAndNewlines), language: language, transcribeMs: elapsed)
    }
}

enum SpeechError: LocalizedError {
    case unsupported

    var errorDescription: String? { "Apple Speech can't transcribe on this iPhone" }
}
