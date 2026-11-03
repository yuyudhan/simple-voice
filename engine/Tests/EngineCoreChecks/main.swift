// FilePath: engine/Tests/EngineCoreChecks/main.swift
// Checks of EngineCore's decisions, run by `just test` (`swift run engine-core-checks`). A plain
// executable rather than an XCTest or swift-testing bundle, so it runs the same with the Command
// Line Tools alone as with Xcode. Exits non-zero when any check fails.

import Foundation

/// Failed checks so far, with where they were made. Top-level code runs on the main actor.
var failures: [String] = []

@MainActor func check(
    _ condition: Bool, _ message: @autoclosure () -> String, file: StaticString = #fileID, line: UInt = #line
) {
    if !condition {
        failures.append("\(file):\(line): \(message())")
    }
}

checkSpellingGate()
await checkPromptCache()

if failures.isEmpty {
    print("engine-core-checks: OK")
} else {
    for failure in failures {
        print("FAIL \(failure)")
    }
    print("engine-core-checks: \(failures.count) check(s) failed")
    exit(1)
}
