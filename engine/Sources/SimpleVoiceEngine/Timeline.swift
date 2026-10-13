// FilePath: engine/Sources/SimpleVoiceEngine/Timeline.swift
// Groups word timings into the timed transcript segments `transcribe` returns with `timestamps`.

import Foundation

/// One recognised word and where it was spoken, in seconds from the start of the audio.
struct TimedWord: Sendable, Equatable {
    let text: String
    let start: TimeInterval
    let end: TimeInterval
}

enum Timeline {
    /// A pause this long between words starts a new segment.
    private static let pauseSplit: TimeInterval = 0.8
    /// Segments end at a sentence end once they are this long, and never grow past the limit.
    private static let sentenceSplit: TimeInterval = 4
    private static let maximumLength: TimeInterval = 15

    /// Sentence-sized segments: split at long pauses, at sentence ends once a segment has some
    /// length, and before a segment grows longer than `maximumLength`. Every word lands in exactly
    /// one segment, in order, so the segments joined with spaces read as the transcript.
    static func segments(from words: [TimedWord]) -> [TimedSegment] {
        var segments: [TimedSegment] = []
        var current: [TimedWord] = []

        func flush() {
            guard let first = current.first, let last = current.last else { return }
            let text = current.map(\.text).joined(separator: " ")
            segments.append(TimedSegment(startMs: milliseconds(first.start), endMs: milliseconds(last.end), text: text))
            current.removeAll(keepingCapacity: true)
        }

        for word in words {
            let text = word.text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty else { continue }
            if let first = current.first, let last = current.last {
                let paused = word.start - last.end >= pauseSplit
                let tooLong = word.end - first.start > maximumLength
                if paused || tooLong {
                    flush()
                }
            }
            current.append(TimedWord(text: text, start: word.start, end: max(word.end, word.start)))
            if let first = current.first, endsSentence(text), word.end - first.start >= sentenceSplit {
                flush()
            }
        }
        flush()
        return segments
    }

    static func milliseconds(_ seconds: TimeInterval) -> Int64 {
        guard seconds.isFinite, seconds > 0 else { return 0 }
        return Int64((seconds * 1000).rounded())
    }

    private static func endsSentence(_ word: String) -> Bool {
        guard let last = word.last else { return false }
        return ".?!।…".contains(last)
    }
}
