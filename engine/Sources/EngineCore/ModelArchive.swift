// FilePath: engine/Sources/EngineCore/ModelArchive.swift
// Models published as one tar rather than a Hugging Face folder (Fluid Speech Pico). The catalog
// pins the archive's SHA-256, so the helper only unpacks bytes that are exactly what it lists.

import CryptoKit
import Foundation

public enum ModelArchive {
    /// Bytes hashed per read, so a large archive never sits in memory whole.
    public static let readChunkBytes = 4 << 20

    public struct Failure: Error, CustomStringConvertible {
        public let description: String
    }

    /// Lowercase hex SHA-256 of the file's bytes.
    public static func sha256Hex(of file: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: file)
        defer { try? handle.close() }
        var hasher = SHA256()
        while let chunk = try handle.read(upToCount: readChunkBytes), !chunk.isEmpty {
            hasher.update(data: chunk)
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    /// Throws unless the file's SHA-256 is `sha256` (hex, either case).
    public static func verify(_ file: URL, sha256: String) throws {
        let actual = try sha256Hex(of: file)
        guard actual == sha256.lowercased() else {
            throw Failure(
                description: "\(file.lastPathComponent) has SHA-256 \(actual), expected \(sha256.lowercased())")
        }
    }

    /// Unpacks a tar whose entries share one top-level folder into `directory`, dropping that
    /// folder so the files land at the top of the model directory. bsdtar refuses entries that
    /// would escape `directory`.
    public static func extract(_ archive: URL, into directory: URL) throws {
        let tar = Process()
        tar.executableURL = URL(fileURLWithPath: "/usr/bin/tar")
        tar.arguments = ["-xf", archive.path, "-C", directory.path, "--strip-components", "1"]
        let errors = Pipe()
        tar.standardError = errors
        try tar.run()
        let message = String(decoding: errors.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        tar.waitUntilExit()
        guard tar.terminationStatus == 0 else {
            let detail = message.trimmingCharacters(in: .whitespacesAndNewlines)
            throw Failure(
                description:
                    "tar could not unpack \(archive.lastPathComponent) (exit \(tar.terminationStatus)): \(detail)")
        }
    }
}
