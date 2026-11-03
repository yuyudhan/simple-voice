// FilePath: engine/Tests/EngineCoreChecks/SpellingGateChecks.swift
// The booster's spelling gate on the shapes of replacements measured on real recordings, with
// invented names: without it the rescorer took own-voice WER from 7.0% to 25.7%.

import EngineCore

@MainActor func checkSpellingGate() {
    let refused = [
        ("Press control", "Robosienzo"),
        ("Kafka", "Kalvi"),
        ("get pods", "AirPods"),
        ("again", "LangChain"),
        // Two names run together never collapse into one of them.
        ("Ajvarhitanshu", "Hitanshu"),
        ("", "Sienzo"),
        ("sienzi", "..."),
    ]
    for (original, replacement) in refused {
        check(
            !SpellingGate.keepsEveryWord(original, replacedBy: replacement),
            "\"\(replacement)\" must not replace \"\(original)\"")
    }
    let corrected = [
        ("sienzi", "Sienzo"),
        ("grok", "Groq"),
        ("Nishan", "Nishant"),
        ("Kulvi,", "Kalvi"),
    ]
    for (original, replacement) in corrected {
        check(
            SpellingGate.keepsEveryWord(original, replacedBy: replacement),
            "\"\(replacement)\" should replace \"\(original)\"")
    }
}
