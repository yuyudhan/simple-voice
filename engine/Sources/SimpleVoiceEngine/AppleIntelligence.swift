// FilePath: engine/Sources/SimpleVoiceEngine/AppleIntelligence.swift
// On-device post-processing with Apple Foundation Models (macOS 26+).

import Foundation
import FoundationModels

@available(macOS 26, *)
enum AppleIntelligence {
    static func status() -> ModelStatusResult {
        if let reason = unavailableReason() {
            return .unsupported(reason)
        }
        return .ready()
    }

    /// Loads the system model ahead of the first polish request.
    static func prewarm() throws {
        if let reason = unavailableReason() {
            throw EngineError(reason)
        }
        LanguageModelSession().prewarm()
    }

    /// Few-shot examples go into the session transcript as real prompt/response turns, which the
    /// model follows more reliably than examples pasted into the instructions.
    static func polish(_ params: PolishParams) async throws -> PolishResult {
        if let reason = unavailableReason() {
            throw EngineError(reason)
        }
        var entries: [Transcript.Entry] = [
            .instructions(Transcript.Instructions(segments: [text(params.system)], toolDefinitions: []))
        ]
        for shot in params.shots {
            entries.append(.prompt(Transcript.Prompt(segments: [text(shot.user)])))
            entries.append(.response(Transcript.Response(assetIDs: [], segments: [text(shot.assistant)])))
        }
        let session = LanguageModelSession(transcript: Transcript(entries: entries))
        do {
            let response = try await session.respond(to: params.user, options: GenerationOptions(temperature: 0))
            return PolishResult(text: response.content, finished: true)
        } catch let error as LanguageModelSession.GenerationError {
            throw EngineError(message(for: error))
        }
    }

    private static func text(_ content: String) -> Transcript.Segment {
        .text(Transcript.TextSegment(content: content))
    }

    private static func unavailableReason() -> String? {
        switch SystemLanguageModel.default.availability {
        case .available:
            return nil
        case .unavailable(.deviceNotEligible):
            return "This Mac does not support Apple Intelligence"
        case .unavailable(.appleIntelligenceNotEnabled):
            return "Apple Intelligence is turned off; turn it on in System Settings → Apple Intelligence & Siri"
        case .unavailable(.modelNotReady):
            return "The Apple Intelligence model is still downloading; try again later"
        case .unavailable:
            return "Apple Intelligence is unavailable"
        }
    }

    private static func message(for error: LanguageModelSession.GenerationError) -> String {
        switch error {
        case .guardrailViolation:
            return "Apple Intelligence declined the text (safety guardrails)"
        case .exceededContextWindowSize:
            return "The text is too long for Apple Intelligence"
        case .assetsUnavailable:
            return "The Apple Intelligence model is unavailable right now"
        case .unsupportedLanguageOrLocale:
            return "Apple Intelligence does not support this language (Hindi and Hinglish included); "
                + "use Groq or Custom formatting"
        case .rateLimited:
            return "Apple Intelligence is busy; try again"
        case .concurrentRequests:
            return "Apple Intelligence is already handling a request"
        case .refusal:
            return "Apple Intelligence refused to rewrite the text"
        default:
            return "Apple Intelligence failed: \(describe(error))"
        }
    }
}
