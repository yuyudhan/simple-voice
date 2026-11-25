// FilePath: engine/Sources/EngineCore/HubTransfer.swift
// How a model download reads the replies of Hugging Face (or a publisher's own host): which replies
// are usable, which are worth retrying, and what the user is told when one is not.
//
// huggingface.co sits behind an AWS WAF. A client it flags gets HTTP 202 with an
// `x-amzn-waf-action` header and an empty body instead of the requested data; a captive portal or
// filtering proxy may answer 200 with its own HTML page. Neither may reach a JSON decoder or be
// saved as a model file (T2794).

import Foundation

/// A download step that did not get what it asked for, worded for the user.
public struct DownloadFailure: Error, CustomStringConvertible, Sendable {
    public let message: String
    /// Whether trying again later may succeed (a challenge, a rate limit, a server hiccup).
    public let isTransient: Bool
    /// The delay the server asked for, if it named one.
    public let retryAfter: Duration?

    public init(_ message: String, isTransient: Bool, retryAfter: Duration? = nil) {
        self.message = message
        self.isTransient = isTransient
        self.retryAfter = retryAfter
    }

    public var description: String { message }
}

public enum HubReply {
    /// Why `response` cannot be used as the body of `what`, or nil when it can. Only HTTP 200 is a
    /// whole answer: a 202 is the WAF's challenge and a 206 is part of a file.
    public static func failure(for response: HTTPURLResponse, what: String) -> DownloadFailure? {
        let status = response.statusCode
        let host = response.url?.host ?? "the server"
        if response.value(forHTTPHeaderField: "x-amzn-waf-action") != nil || status == 202 {
            return DownloadFailure(
                "\(host) held back \(what) with a bot check (HTTP \(status)); this usually clears within "
                    + "minutes, otherwise try another network or turn a VPN off",
                isTransient: true)
        }
        switch status {
        case 200:
            return nil
        case 429:
            return DownloadFailure(
                "\(host) is limiting downloads (HTTP 429) while sending \(what)", isTransient: true,
                retryAfter: retryAfter(response))
        case 408, 500, 502, 503, 504:
            return DownloadFailure(
                "\(host) had a temporary problem (HTTP \(status)) sending \(what)", isTransient: true,
                retryAfter: retryAfter(response))
        case 401, 403:
            return DownloadFailure(
                "\(host) refused \(what) (HTTP \(status)); a proxy or network filter may be blocking it",
                isTransient: false)
        case 404, 410:
            return DownloadFailure(
                "\(host) no longer has \(what) (HTTP \(status)); update Simple Voice", isTransient: false)
        default:
            return DownloadFailure("\(host) answered HTTP \(status) for \(what)", isTransient: false)
        }
    }

    /// Checks that a fetched file has the size the listing promised; a short or substituted file
    /// is deleted so it is never installed or kept for a retry.
    public static func verifySize(of file: URL, bytes: Int64, what: String) throws {
        let attributes = try? FileManager.default.attributesOfItem(atPath: file.path)
        let received = (attributes?[.size] as? NSNumber)?.int64Value ?? 0
        guard received != bytes else { return }
        try? FileManager.default.removeItem(at: file)
        throw DownloadFailure("received \(received) of \(bytes) bytes of \(what)", isTransient: true)
    }

    private static func retryAfter(_ response: HTTPURLResponse) -> Duration? {
        guard let text = response.value(forHTTPHeaderField: "Retry-After"),
            let seconds = Int(text.trimmingCharacters(in: .whitespaces)), seconds >= 0
        else { return nil }
        return .seconds(seconds)
    }
}

/// Repeats a download step after transient failures, waiting 2 s, 4 s, 8 s (or what the server
/// asked for, up to a minute) between attempts.
public struct RetryPolicy: Sendable {
    public let attempts: Int
    let firstDelay: Duration
    let longestDelay: Duration
    let sleep: @Sendable (Duration) async throws -> Void

    public init(
        attempts: Int = 4,
        sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }
    ) {
        self.attempts = attempts
        firstDelay = .seconds(2)
        longestDelay = .seconds(60)
        self.sleep = sleep
    }

    public func run<T>(
        isolation: isolated (any Actor)? = #isolation, _ operation: () async throws -> T
    ) async throws -> T {
        var delay = firstDelay
        var attempt = 1
        while true {
            do {
                return try await operation()
            } catch {
                guard let failure = Self.transient(error) else { throw error }
                guard attempt < attempts else {
                    throw DownloadFailure(
                        "\(failure.message) (gave up after \(attempts) attempts)", isTransient: true)
                }
                try await sleep(min(failure.retryAfter ?? delay, longestDelay))
                delay *= 2
                attempt += 1
            }
        }
    }

    /// The error as a transient failure, or nil when retrying cannot help.
    private static func transient(_ error: any Error) -> DownloadFailure? {
        if let failure = error as? DownloadFailure {
            return failure.isTransient ? failure : nil
        }
        guard let urlError = error as? URLError else { return nil }
        switch urlError.code {
        case .timedOut, .networkConnectionLost, .notConnectedToInternet, .cannotConnectToHost,
            .cannotFindHost, .dnsLookupFailed, .resourceUnavailable, .badServerResponse,
            .secureConnectionFailed:
            let host = urlError.failingURL?.host ?? "the server"
            return DownloadFailure(
                "the connection to \(host) failed: \(urlError.localizedDescription)", isTransient: true)
        default:
            return nil
        }
    }
}

/// One entry of the Hugging Face tree API. LFS files (today xet-backed, also carrying `xetHash`)
/// report their size under `lfs.size` as well as `size`.
public struct HubListingEntry: Decodable, Sendable {
    public struct LargeFile: Decodable, Sendable {
        public let size: Int64
    }

    public let type: String
    public let path: String
    public let size: Int64?
    public let lfs: LargeFile?

    public var byteCount: Int64 { lfs?.size ?? size ?? 0 }
}

public enum HubListing {
    /// Every entry of the listing at `first`, following `Link: rel="next"` pages. Each page is
    /// retried on its own under `policy`.
    public static func entries(
        from first: URL, repo: String, policy: RetryPolicy,
        isolation: isolated (any Actor)? = #isolation,
        fetch: (URL) async throws -> (Data, URLResponse)
    ) async throws -> [HubListingEntry] {
        var entries: [HubListingEntry] = []
        var next: URL? = first
        while let url = next {
            let (page, response) = try await policy.run { () async throws -> ([HubListingEntry], HTTPURLResponse) in
                let (data, response) = try await fetch(url)
                guard let http = response as? HTTPURLResponse else {
                    throw DownloadFailure("\(url.host ?? "the server") did not answer over HTTP", isTransient: true)
                }
                let what = "the file list of \(repo)"
                if let failure = HubReply.failure(for: http, what: what) { throw failure }
                return (try decode(data, response: http, what: what), http)
            }
            entries += page
            next = nextPage(response)
        }
        return entries
    }

    /// Decodes one page; anything else (an interception page, a changed API) is described by its
    /// type, size and first characters rather than by the decoder's generic complaint.
    static func decode(_ data: Data, response: HTTPURLResponse, what: String) throws -> [HubListingEntry] {
        do {
            return try JSONDecoder().decode([HubListingEntry].self, from: data)
        } catch {
            let type = response.value(forHTTPHeaderField: "Content-Type") ?? "no content type"
            let head = String(decoding: data.prefix(60), as: UTF8.self)
                .components(separatedBy: .whitespacesAndNewlines).filter { !$0.isEmpty }.joined(separator: " ")
            let body = data.isEmpty ? "an empty body" : "\(data.count) bytes starting \"\(head)\""
            let host = response.url?.host ?? "the server"
            throw DownloadFailure(
                "\(host) sent something that is not the file list (HTTP \(response.statusCode), \(type), \(body)) "
                    + "for \(what); a proxy, captive portal or network filter may be intercepting it",
                isTransient: true)
        }
    }

    /// The listing API pages large repositories with `Link: <url>; rel="next"`.
    static func nextPage(_ response: HTTPURLResponse) -> URL? {
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
