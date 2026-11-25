// FilePath: engine/Tests/EngineCoreChecks/HubTransferChecks.swift
// Hugging Face replies that are not what a download asked for (T2794). The AWS WAF in front of
// huggingface.co answers a flagged client with HTTP 202, `x-amzn-waf-action: challenge` and an empty
// body; decoding that as the file list surfaced as Foundation's "The data couldn't be read because
// it isn't in the correct format". These replay such replies through the listing and retry logic.

import EngineCore
import Foundation

@MainActor func checkHubTransfer() async {
    checkReplyVerdicts()
    checkFileSize()
    await checkListing()
    await checkRetries()
}

// Falls back to a URL no check accepts, so a typo fails the checks instead of crashing them.
private let listingURL =
    URL(string: "https://huggingface.co/api/models/FluidInference/parakeet-tdt-0.6b-v3-coreml/tree/main?recursive=true")
    ?? URL(fileURLWithPath: "/")

/// The shape of today's listing: plain files plus xet-backed LFS files carrying `xetHash`.
private let listingJSON = """
    [{"type":"directory","oid":"a1","size":0,"path":"Decoder.mlmodelc"},
     {"type":"file","oid":"b2","size":243,"path":"Decoder.mlmodelc/coremldata.bin"},
     {"type":"file","oid":"c3","size":23592960,"path":"Decoder.mlmodelc/weights/weight.bin",
      "lfs":{"oid":"d4","size":23592960,"pointerSize":134},"xetHash":"e5"}]
    """

private func reply(
    _ status: Int, _ headers: [String: String] = [:], url: URL = listingURL
) -> HTTPURLResponse {
    HTTPURLResponse(url: url, statusCode: status, httpVersion: "HTTP/1.1", headerFields: headers)
        ?? HTTPURLResponse()
}

private let wafChallenge = reply(202, ["x-amzn-waf-action": "challenge", "Content-Length": "0"])
private let jsonOK = reply(200, ["Content-Type": "application/json; charset=utf-8"])

/// Replays canned replies in order and records the requested URLs.
private final class Replay: @unchecked Sendable {
    private let lock = NSLock()
    private var replies: [(Data, HTTPURLResponse)]
    private(set) var requested: [URL] = []

    init(_ replies: [(Data, HTTPURLResponse)]) {
        self.replies = replies
    }

    func fetch(_ url: URL) throws -> (Data, URLResponse) {
        try lock.withLock {
            requested.append(url)
            guard !replies.isEmpty else { throw URLError(.resourceUnavailable) }
            return replies.removeFirst()
        }
    }
}

/// Records the backoff delays instead of sleeping.
private final class Sleeps: @unchecked Sendable {
    private let lock = NSLock()
    private var recorded: [Duration] = []

    var delays: [Duration] { lock.withLock { recorded } }

    func sleep(_ delay: Duration) {
        lock.withLock { recorded.append(delay) }
    }
}

@MainActor private func checkReplyVerdicts() {
    check(HubReply.failure(for: jsonOK, what: "x") == nil, "HTTP 200 is accepted")

    let waf = HubReply.failure(for: wafChallenge, what: "the file list")
    check(waf?.isTransient == true, "a WAF challenge is retried")
    check(waf?.description.contains("HTTP 202") == true, "the WAF message names the status: \(String(describing: waf))")
    check(
        HubReply.failure(for: reply(200, ["x-amzn-waf-action": "captcha"]), what: "x")?.isTransient == true,
        "a WAF action header is a challenge whatever the status")

    let limited = HubReply.failure(for: reply(429, ["Retry-After": "7"]), what: "x")
    check(limited?.isTransient == true, "HTTP 429 is retried")
    check(limited?.retryAfter == .seconds(7), "Retry-After is honoured: \(String(describing: limited?.retryAfter))")
    for status in [408, 500, 502, 503, 504] {
        check(HubReply.failure(for: reply(status), what: "x")?.isTransient == true, "HTTP \(status) is retried")
    }

    let missing = HubReply.failure(for: reply(404), what: "Encoder.mlmodelc/weight.bin")
    check(missing?.isTransient == false, "HTTP 404 is not retried")
    check(
        missing?.description.contains("HTTP 404") == true && missing?.description.contains("weight.bin") == true,
        "a 404 names the status and the file: \(String(describing: missing))")
    check(HubReply.failure(for: reply(401), what: "x")?.isTransient == false, "HTTP 401 is not retried")
    check(HubReply.failure(for: reply(206), what: "x") != nil, "a partial reply is not a whole file")
}

@MainActor private func checkFileSize() {
    let file = FileManager.default.temporaryDirectory.appendingPathComponent("hub-size-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: file) }
    do {
        try Data(count: 10).write(to: file)
        try HubReply.verifySize(of: file, bytes: 10, what: "a.bin")
        do {
            try HubReply.verifySize(of: file, bytes: 4096, what: "a.bin")
            check(false, "a short file must throw")
        } catch let failure as DownloadFailure {
            check(failure.isTransient, "a short file is retried")
            check(failure.description.contains("10 of 4096 bytes"), "the size message: \(failure)")
            check(!FileManager.default.fileExists(atPath: file.path), "a short file is deleted, never kept")
        }
    } catch {
        check(false, "file size checks threw \(error)")
    }
}

@MainActor private func checkListing() async {
    let sleeps = Sleeps()
    let policy = RetryPolicy(sleep: { sleeps.sleep($0) })

    // The reported failure: the bot check first, then the real listing.
    let recovering = Replay([(Data(), wafChallenge), (Data(listingJSON.utf8), jsonOK)])
    do {
        let entries = try await HubListing.entries(
            from: listingURL, repo: "FluidInference/parakeet-tdt-0.6b-v3-coreml", policy: policy,
            fetch: { try recovering.fetch($0) })
        check(entries.count == 3, "the listing decodes after the challenge clears: \(entries.count)")
        check(entries.last?.byteCount == 23_592_960, "an xet-backed LFS file counts its real size")
        check(recovering.requested.count == 2, "the challenged request is repeated once")
    } catch {
        check(false, "a passing WAF challenge must not fail the download: \(error)")
    }

    // A challenge that never clears ends in a message about the challenge, not a decoding error.
    let blocked = Replay(Array(repeating: (Data(), wafChallenge), count: 4))
    await expectFailure(
        containing: ["HTTP 202", "4 attempts"], from: "a lasting WAF challenge",
        excluding: "correct format"
    ) {
        _ = try await HubListing.entries(
            from: listingURL, repo: "r", policy: policy, fetch: { try blocked.fetch($0) })
    }

    // A captive portal or filtering proxy answering 200 with its own page.
    let page = Data("<!DOCTYPE html><html><title>Access blocked</title></html>".utf8)
    let portal = Replay(Array(repeating: (page, reply(200, ["Content-Type": "text/html"])), count: 4))
    await expectFailure(
        containing: ["not the file list", "text/html", "<!DOCTYPE html>"], from: "an HTML page",
        excluding: "correct format"
    ) {
        _ = try await HubListing.entries(
            from: listingURL, repo: "r", policy: policy, fetch: { try portal.fetch($0) })
    }

    // Pages are followed through `Link: rel="next"`.
    let next = "https://huggingface.co/api/models/r/tree/main?recursive=true&cursor=abc"
    let paged = Replay([
        (Data(listingJSON.utf8), reply(200, ["Content-Type": "application/json", "Link": "<\(next)>; rel=\"next\""])),
        (Data(listingJSON.utf8), jsonOK),
    ])
    let all = try? await HubListing.entries(from: listingURL, repo: "r", policy: policy, fetch: { try paged.fetch($0) })
    check(all?.count == 6, "both pages are listed")
    check(paged.requested.last?.absoluteString == next, "the second request follows the Link header")
}

@MainActor private func checkRetries() async {
    let sleeps = Sleeps()
    let policy = RetryPolicy(sleep: { sleeps.sleep($0) })

    // Network drops are retried with doubling delays; a 404 is not retried at all.
    var calls = 0
    let value = try? await policy.run { () async throws -> Int in
        calls += 1
        if calls < 3 { throw URLError(.networkConnectionLost) }
        return calls
    }
    check(value == 3, "the third attempt succeeds")
    check(sleeps.delays == [.seconds(2), .seconds(4)], "backoff doubles: \(sleeps.delays)")

    calls = 0
    await expectFailure(containing: ["HTTP 404"], from: "a 404", excluding: "attempts") {
        _ = try await policy.run { () async throws -> Int in
            calls += 1
            throw HubReply.failure(for: reply(404), what: "x") ?? DownloadFailure("HTTP 200", isTransient: false)
        }
    }
    check(calls == 1, "a permanent failure is tried once: \(calls)")
}

@MainActor private func expectFailure(
    containing parts: [String], from label: String, excluding unwanted: String,
    _ body: () async throws -> Void
) async {
    do {
        try await body()
        check(false, "\(label) must fail")
    } catch {
        let text = "\(error)"
        for part in parts {
            check(text.contains(part), "\(label): the message names \(part): \(text)")
        }
        check(!text.contains(unwanted), "\(label): the message is not \"\(unwanted)\": \(text)")
    }
}
