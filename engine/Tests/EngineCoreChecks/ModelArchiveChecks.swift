// FilePath: engine/Tests/EngineCoreChecks/ModelArchiveChecks.swift
// A model shipped as one tar (Fluid Speech Pico): its pinned digest is checked across several read
// chunks, and its single top-level folder is stripped so the files land at the top of the model
// directory, as the Hugging Face downloads put them.

import EngineCore
import Foundation

@MainActor func checkModelArchive() {
    let scratch = FileManager.default.temporaryDirectory
        .appendingPathComponent("engine-core-checks-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: scratch) }
    do {
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
        try checkDigest(in: scratch)
        try checkExtraction(in: scratch)
    } catch {
        check(false, "model archive checks threw \(error)")
    }
}

@MainActor private func checkDigest(in scratch: URL) throws {
    let small = scratch.appendingPathComponent("abc.bin")
    try Data("abc".utf8).write(to: small)
    check(
        try ModelArchive.sha256Hex(of: small)
            == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "SHA-256 of \"abc\"")

    // Three and a half read chunks of "a": the digest must cover every chunk, not just the first.
    let large = scratch.appendingPathComponent("a.bin")
    try Data(repeating: UInt8(ascii: "a"), count: ModelArchive.readChunkBytes * 7 / 2).write(to: large)
    let expected = try ModelArchive.sha256Hex(of: large)
    check(expected.count == 64, "a SHA-256 is 64 hex digits")
    let truncated = scratch.appendingPathComponent("a-short.bin")
    try Data(repeating: UInt8(ascii: "a"), count: ModelArchive.readChunkBytes * 3).write(to: truncated)
    check(try ModelArchive.sha256Hex(of: truncated) != expected, "a truncated file has another digest")

    do {
        try ModelArchive.verify(large, sha256: String(repeating: "0", count: 64))
        check(false, "a digest mismatch must throw")
    } catch {
        check("\(error)".contains("SHA-256"), "the mismatch names the digest: \(error)")
    }
    try ModelArchive.verify(large, sha256: expected.uppercased())
}

@MainActor private func checkExtraction(in scratch: URL) throws {
    let source = scratch.appendingPathComponent("source/pack-coreml/Encoder-1.mlmodelc", isDirectory: true)
    try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)
    try Data("weights".utf8).write(to: source.appendingPathComponent("weight.bin"))
    try Data("{}".utf8).write(
        to: scratch.appendingPathComponent("source/pack-coreml/parakeet_vocab.json"))
    let archive = scratch.appendingPathComponent("pack.tar")
    let tar = Process()
    tar.executableURL = URL(fileURLWithPath: "/usr/bin/tar")
    tar.arguments = ["-cf", archive.path, "-C", scratch.appendingPathComponent("source").path, "pack-coreml"]
    try tar.run()
    tar.waitUntilExit()
    check(tar.terminationStatus == 0, "the fixture tar was created")

    let target = scratch.appendingPathComponent("model", isDirectory: true)
    try FileManager.default.createDirectory(at: target, withIntermediateDirectories: true)
    try ModelArchive.extract(archive, into: target)
    let weights = target.appendingPathComponent("Encoder-1.mlmodelc/weight.bin")
    check(
        (try? String(contentsOf: weights, encoding: .utf8)) == "weights",
        "the top folder is stripped: Encoder-1.mlmodelc sits at the top of the model directory")
    check(
        FileManager.default.fileExists(atPath: target.appendingPathComponent("parakeet_vocab.json").path),
        "every file is extracted")
    check(
        !FileManager.default.fileExists(atPath: target.appendingPathComponent("pack-coreml").path),
        "no wrapper folder is left")

    do {
        try ModelArchive.extract(scratch.appendingPathComponent("missing.tar"), into: target)
        check(false, "extracting a missing archive must throw")
    } catch {
        check("\(error)".contains("tar"), "the failure names tar: \(error)")
    }
}
