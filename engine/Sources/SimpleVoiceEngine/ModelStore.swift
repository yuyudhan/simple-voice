// FilePath: engine/Sources/SimpleVoiceEngine/ModelStore.swift
// Parakeet model files on disk: status, Hugging Face downloads and deletion.
//
// Layout: `<models-dir>/<model-id>/` holds exactly the files the loader needs plus a completion
// marker. Downloads land in `<model-id>.partial/` and are renamed into place only once every file
// is present, so a crash or a lost connection never leaves a directory that looks ready.

import Foundation

/// The subset of a Hugging Face repository one local model needs.
struct HubModel: Sendable {
    let id: ModelID
    let repo: String
    /// Repository folder holding the variant (empty for the repository root).
    let subdirectory: String
    /// Files or `.mlmodelc` directories, relative to `subdirectory`.
    let entries: [String]

    static func forModel(_ id: ModelID) -> HubModel? {
        switch id {
        case .parakeetTdtV3:
            HubModel(
                id: id,
                repo: "FluidInference/parakeet-tdt-0.6b-v3-coreml",
                subdirectory: "",
                entries: [
                    "Preprocessor.mlmodelc", "Encoder.mlmodelc", "Decoder.mlmodelc",
                    "JointDecisionv3.mlmodelc", "parakeet_vocab.json",
                ]
            )
        case .parakeetTdtV2:
            HubModel(
                id: id,
                repo: "FluidInference/parakeet-tdt-0.6b-v2-coreml",
                subdirectory: "",
                entries: [
                    "Preprocessor.mlmodelc", "Encoder.mlmodelc", "Decoder.mlmodelc",
                    "JointDecision.mlmodelc", "parakeet_vocab.json",
                ]
            )
        case .parakeetFlash:
            // The 1280 ms variant: the helper transcribes whole recordings, where the largest
            // chunk gives the best accuracy and throughput.
            HubModel(
                id: id,
                repo: "FluidInference/parakeet-realtime-eou-120m-coreml",
                subdirectory: "1280ms",
                entries: ["streaming_encoder.mlmodelc", "decoder.mlmodelc", "joint_decision.mlmodelc", "vocab.json"]
            )
        case .appleSpeech, .appleIntelligence:
            nil
        }
    }

    /// Maps a repository path to its path inside the model directory, or nil when not needed.
    func localPath(forRepoPath repoPath: String) -> String? {
        let prefix = subdirectory.isEmpty ? "" : subdirectory + "/"
        guard repoPath.hasPrefix(prefix) else { return nil }
        let relative = String(repoPath.dropFirst(prefix.count))
        let wanted = entries.contains { relative == $0 || relative.hasPrefix($0 + "/") }
        return wanted ? relative : nil
    }
}

actor ModelStore {
    private static let markerName = ".simple-voice-complete"

    let root: URL
    private var downloads: [ModelID: Task<Void, any Error>] = [:]

    init(root: URL) {
        self.root = root
    }

    nonisolated func directory(for id: ModelID) -> URL {
        root.appendingPathComponent(id.rawValue, isDirectory: true)
    }

    nonisolated func status(of model: HubModel) -> ModelStatusResult {
        guard let size = completedSize(of: model) else { return .notDownloaded() }
        return .ready(sizeBytes: size)
    }

    /// The model directory, or an error telling the user to download the model first.
    nonisolated func readyDirectory(for model: HubModel) throws -> URL {
        guard completedSize(of: model) != nil else {
            throw EngineError("\(model.id.rawValue) is not downloaded; download it in Settings → Models")
        }
        return directory(for: model.id)
    }

    /// Downloads the model unless it is already complete. A second request for a model that is
    /// already downloading waits for the first one instead of starting another transfer.
    func download(_ model: HubModel, progress: ProgressSink) async throws {
        if completedSize(of: model) != nil { return }
        if let running = downloads[model.id] {
            try await running.value
            return
        }
        let destination = directory(for: model.id)
        let partial = root.appendingPathComponent(model.id.rawValue + ".partial", isDirectory: true)
        let task = Task {
            try await HubDownload(model: model, destination: destination, partial: partial, progress: progress).run()
        }
        downloads[model.id] = task
        do {
            try await task.value
            downloads[model.id] = nil
        } catch {
            downloads[model.id] = nil
            throw error
        }
    }

    func delete(_ model: HubModel) throws {
        let manager = FileManager.default
        let targets = [
            directory(for: model.id),
            root.appendingPathComponent(model.id.rawValue + ".partial", isDirectory: true),
        ]
        for target in targets where manager.fileExists(atPath: target.path) {
            try manager.removeItem(at: target)
        }
    }

    private nonisolated func completedSize(of model: HubModel) -> Int64? {
        let marker = directory(for: model.id).appendingPathComponent(Self.markerName)
        guard let text = try? String(contentsOf: marker, encoding: .utf8) else { return nil }
        return Int64(text.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    fileprivate static func writeMarker(in directory: URL, totalBytes: Int64) throws {
        let marker = directory.appendingPathComponent(markerName)
        try String(totalBytes).write(to: marker, atomically: true, encoding: .utf8)
    }
}

// MARK: - Hugging Face transfer

private struct HubEntry: Decodable {
    struct LargeFile: Decodable {
        let size: Int64
    }

    let type: String
    let path: String
    let size: Int64?
    let lfs: LargeFile?

    var byteCount: Int64 { lfs?.size ?? size ?? 0 }
}

private struct PlannedFile {
    let remotePath: String
    let localPath: String
    let size: Int64
}

private struct HubDownload {
    let model: HubModel
    let destination: URL
    let partial: URL
    let progress: ProgressSink

    func run() async throws {
        let files = try await plan()
        let total = files.reduce(Int64(0)) { $0 + $1.size }
        let manager = FileManager.default
        try manager.createDirectory(at: partial, withIntermediateDirectories: true)

        let counter = ByteCounter(total: total, progress: progress)
        let transfer = FileTransfer(counter: counter)
        let session = URLSession(configuration: .default, delegate: transfer, delegateQueue: nil)
        defer { session.finishTasksAndInvalidate() }

        for file in files {
            let target = partial.appendingPathComponent(file.localPath)
            // Files left complete by an interrupted attempt are kept, so a retry resumes.
            if let attributes = try? manager.attributesOfItem(atPath: target.path),
                (attributes[.size] as? NSNumber)?.int64Value == file.size
            {
                counter.finishFile(file.size)
                continue
            }
            try Task.checkCancellation()
            let url = try resolveURL(file.remotePath)
            try await transfer.fetch(url, to: target, using: session)
            counter.finishFile(file.size)
        }

        await progress.report(1, "Finishing", force: true)
        try ModelStore.writeMarker(in: partial, totalBytes: total)
        if manager.fileExists(atPath: destination.path) {
            try manager.removeItem(at: destination)
        }
        try manager.moveItem(at: partial, to: destination)
        Log.info("downloaded \(model.id.rawValue) (\(total) bytes)")
    }

    /// Lists the repository and keeps only the files the loader reads.
    private func plan() async throws -> [PlannedFile] {
        var entries: [HubEntry] = []
        var next: URL? = try listingURL()
        while let url = next {
            let (data, response) = try await URLSession.shared.data(from: url)
            let http = response as? HTTPURLResponse
            guard let http, (200..<300).contains(http.statusCode) else {
                throw EngineError("Hugging Face listing failed (HTTP \(http?.statusCode ?? 0)) for \(model.repo)")
            }
            entries += try JSONDecoder().decode([HubEntry].self, from: data)
            next = nextPage(http)
        }

        let files = entries.compactMap { entry -> PlannedFile? in
            guard entry.type == "file", let local = model.localPath(forRepoPath: entry.path) else { return nil }
            return PlannedFile(remotePath: entry.path, localPath: local, size: entry.byteCount)
        }
        for required in model.entries {
            let present = files.contains { $0.localPath == required || $0.localPath.hasPrefix(required + "/") }
            if !present {
                throw EngineError("\(model.repo) no longer contains \(required); update Simple Voice")
            }
        }
        return files
    }

    private func listingURL() throws -> URL {
        let folder = model.subdirectory.isEmpty ? "" : "/" + model.subdirectory
        let text = "https://huggingface.co/api/models/\(model.repo)/tree/main\(folder)?recursive=true"
        guard let url = URL(string: text) else { throw EngineError("invalid listing URL \(text)") }
        return url
    }

    private func resolveURL(_ path: String) throws -> URL {
        let encoded = path.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? path
        let text = "https://huggingface.co/\(model.repo)/resolve/main/\(encoded)"
        guard let url = URL(string: text) else { throw EngineError("invalid download URL \(text)") }
        return url
    }

    /// The listing API pages large repositories with `Link: <url>; rel="next"`.
    private func nextPage(_ response: HTTPURLResponse) -> URL? {
        guard let link = response.value(forHTTPHeaderField: "Link") else { return nil }
        for part in link.split(separator: ",") where part.contains("rel=\"next\"") {
            guard let open = part.firstIndex(of: "<"), let close = part.firstIndex(of: ">"), open < close else {
                continue
            }
            return URL(string: String(part[part.index(after: open)..<close]))
        }
        return nil
    }
}

/// Aggregates bytes across all files of one download and forwards throttled progress.
private final class ByteCounter: @unchecked Sendable {
    private let total: Int64
    private let progress: ProgressSink
    private let lock = NSLock()
    private var finished: Int64 = 0
    private var lastReport: ContinuousClock.Instant?

    init(total: Int64, progress: ProgressSink) {
        self.total = total
        self.progress = progress
    }

    func finishFile(_ size: Int64) {
        let done = lock.withLock {
            finished += size
            return finished
        }
        report(done, force: true)
    }

    func currentFile(written: Int64) {
        let done = lock.withLock { finished + written }
        report(done, force: false)
    }

    private func report(_ done: Int64, force: Bool) {
        let now = ContinuousClock.now
        let due = lock.withLock {
            if !force, let lastReport, now - lastReport < .milliseconds(100) {
                return false
            }
            lastReport = now
            return true
        }
        guard due else { return }
        let fraction = total > 0 ? Double(done) / Double(total) : 0
        let message = "Downloading \(Self.megabytes(done)) of \(Self.megabytes(total)) MB"
        let progress = self.progress
        Task { await progress.report(min(fraction, 0.999), message) }
    }

    private static func megabytes(_ bytes: Int64) -> Int64 {
        bytes / 1_000_000
    }
}

/// One file at a time over a delegate-based session, which is what reports bytes as they arrive.
private final class FileTransfer: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {
    private let counter: ByteCounter
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Void, any Error>?
    private var destination: URL?
    private var failure: (any Error)?

    init(counter: ByteCounter) {
        self.counter = counter
    }

    func fetch(_ url: URL, to destination: URL, using session: URLSession) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
            lock.withLock {
                self.continuation = continuation
                self.destination = destination
                self.failure = nil
            }
            session.downloadTask(with: url).resume()
        }
    }

    func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didWriteData bytesWritten: Int64,
        totalBytesWritten: Int64,
        totalBytesExpectedToWrite: Int64
    ) {
        counter.currentFile(written: totalBytesWritten)
    }

    // The temporary file is deleted when this returns, so it is moved synchronously here.
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        let target = lock.withLock { destination }
        do {
            let status = (downloadTask.response as? HTTPURLResponse)?.statusCode ?? 0
            let name = downloadTask.originalRequest?.url?.lastPathComponent ?? "file"
            guard (200..<300).contains(status) else {
                throw EngineError("download of \(name) failed (HTTP \(status))")
            }
            guard let target else { throw EngineError("download of \(name) has no destination") }
            let manager = FileManager.default
            try manager.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
            if manager.fileExists(atPath: target.path) {
                try manager.removeItem(at: target)
            }
            try manager.moveItem(at: location, to: target)
        } catch {
            lock.withLock { failure = error }
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: (any Error)?) {
        let (continuation, failure) = lock.withLock {
            let pending = (self.continuation, self.failure)
            self.continuation = nil
            return pending
        }
        if let problem = error ?? failure {
            continuation?.resume(throwing: problem)
        } else {
            continuation?.resume()
        }
    }
}
