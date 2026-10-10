// FilePath: mobile/src-tauri/gen/apple/SimpleVoiceKeyboard/DictationController.swift
// The mic's side of the keyboard. An iPhone keyboard cannot record (MB-7), so it asks the Simple
// Voice app, which keeps a recording session in the background, through the App Group files in
// SvShared/Bridge.swift: requests go out, replies and the status come back, each announced by a
// Darwin notification. This class turns that exchange into the top bar's text and mic state.

import UIKit

@MainActor
protocol DictationHost: AnyObject {
    var hasFullAccess: Bool { get }
    func showStatus(_ line: String, tone: Tone)
    func showMic(_ state: MicState)
    func showLevel(_ level: Float)
    func insertDictation(_ text: String)
    /// Opens `url` in the app; `completion` says whether that worked.
    func openApp(_ url: URL, completion: @escaping (Bool) -> Void)
}

@MainActor
final class DictationController {
    private enum Phase: Equatable {
        case idle
        /// The app was asked to start a session; mic taps still work in case it never did.
        case opening
        case starting
        /// `start` got no answer in time.
        case stalled
        case recording(since: Date)
        case working(String)
        /// A message shown for a moment, then the status line returns.
        case notice(String, Tone)
    }

    private weak var host: DictationHost?
    private var files: SharedFiles?
    private var status: SharedStatus?
    private var phase = Phase.idle
    /// The history row of the last dictation that failed to send, for Retry (MB-5).
    private var retryId: Int64?
    private var lastRequestId: String?
    private var lastReply: KeyboardReply?
    private var timeout: Timer?
    private var clock: Timer?
    private var meter: Timer?

    private static let startTimeout: TimeInterval = 3
    private static let noticeSeconds: TimeInterval = 2.5
    private static let meterInterval: TimeInterval = 0.08
    private static let defaultMaxSeconds = 300

    init(host: DictationHost) {
        self.host = host
        DarwinNote.observe(Bridge.Note.status) { [weak self] in self?.statusChanged() }
        DarwinNote.observe(Bridge.Note.reply) { [weak self] in self?.replyArrived() }
    }

    /// The keyboard is showing: read the app's latest state, including a reply that arrived
    /// while the keyboard was away.
    func appear() {
        if files == nil { files = SharedFiles() }
        status = files?.readStatus()
        replyArrived()
        render()
    }

    /// Leaving the field mid-recording abandons the recording, like Esc on the Mac (D-3).
    func disappear() {
        switch phase {
        case .starting, .recording:
            send(.cancel)
            setPhase(.idle)
        default:
            break
        }
    }

    func micTapped() {
        switch phase {
        case .recording:
            stop()
            return
        case .starting, .working:
            return
        case .idle, .opening, .stalled, .notice:
            break
        }
        guard let host, host.hasFullAccess, let files else { return }
        // The key may have been added or the session started since the bar last changed.
        status = files.readStatus()
        guard let status, !status.needsKey else {
            if let url = Bridge.settingsURL { host.openApp(url) { _ in } }
            return
        }
        retryId = nil
        guard status.sessionIsLive() else {
            openSession()
            return
        }
        send(.start)
        setPhase(.starting)
        timeout = timer(after: Self.startTimeout) { controller in
            if controller.phase == .starting { controller.setPhase(.stalled) }
        }
    }

    /// After a failed upload the status reads "Not sent · tap to retry"; this is that tap.
    func statusTapped() {
        guard let id = retryId, let files else { return }
        switch phase {
        case .idle, .notice, .opening, .stalled:
            break
        case .starting, .recording, .working:
            return
        }
        status = files.readStatus()
        guard status?.sessionIsLive() == true else {
            // The retry needs the app; start a session and keep the retry for the next tap.
            openSession()
            return
        }
        retryId = nil
        send(.retry, historyId: id)
        setPhase(.working("Retrying…"))
    }

    private func openSession() {
        guard let host, let url = Bridge.sessionURL else { return }
        send(.startSession)
        setPhase(.opening)
        host.openApp(url) { [weak self] opened in
            guard let self, !opened, self.phase == .opening else { return }
            self.notice("Open Simple Voice to start dictating", tone: .blocked)
        }
    }

    private func stop() {
        send(.stop)
        setPhase(.working("Transcribing…"))
    }

    private func send(_ kind: RequestKind, historyId: Int64? = nil) {
        let request = KeyboardRequest(kind: kind, historyId: historyId)
        lastRequestId = request.id
        files?.enqueue(request)
    }

    private func statusChanged() {
        status = files?.readStatus()
        let live = status?.sessionIsLive() == true
        switch phase {
        case .opening where live:
            setPhase(.idle)
        case .starting, .recording, .working:
            // The session ended under us (closed, killed or timed out): nothing will answer.
            if !live { setPhase(.idle) }
        default:
            render()
        }
    }

    private func replyArrived() {
        guard let files, let reply = files.readReply(), reply.requestId == lastRequestId, reply != lastReply
        else { return }
        lastReply = reply
        switch reply.kind {
        case .recording:
            guard phase == .starting || phase == .stalled else { return }
            setPhase(.recording(since: Date(timeIntervalSince1970: reply.at / 1000)))
        case .text:
            setPhase(.idle)
            host?.insertDictation(reply.text ?? "")
            // The ack tells the app the text landed; without it the app copies it instead (D-15).
            send(.ack, historyId: reply.historyId)
        case .noSpeech:
            notice("Nothing heard", tone: .working)
        case .failed:
            if let id = reply.historyId {
                retryId = id
                setPhase(.idle)
            } else {
                notice(reply.message ?? "Dictation failed", tone: .blocked)
            }
        case .error:
            notice(reply.message ?? "Dictation failed", tone: .blocked)
        case .sessionEnded:
            setPhase(.idle)
        }
    }

    private func notice(_ line: String, tone: Tone) {
        setPhase(.notice(line, tone))
        timeout = timer(after: Self.noticeSeconds) { controller in
            if case .notice = controller.phase { controller.setPhase(.idle) }
        }
    }

    private func setPhase(_ next: Phase) {
        timeout?.invalidate()
        timeout = nil
        let wasRecording: Bool
        if case .recording = phase { wasRecording = true } else { wasRecording = false }
        phase = next
        if case .recording = next {
            if !wasRecording { startMeters() }
        } else if wasRecording {
            clock?.invalidate()
            meter?.invalidate()
            clock = nil
            meter = nil
        }
        render()
    }

    private func startMeters() {
        clock = timer(after: 1, repeats: true) { controller in controller.render() }
        meter = timer(after: Self.meterInterval, repeats: true) { controller in
            guard let files = controller.files else { return }
            controller.host?.showLevel(files.level)
        }
    }

    /// A main-run-loop timer that holds the controller weakly. Its callback runs on the main thread,
    /// which `assumeIsolated` states for the compiler.
    private func timer(
        after interval: TimeInterval,
        repeats: Bool = false,
        _ action: @escaping @MainActor @Sendable (DictationController) -> Void
    ) -> Timer {
        Timer.scheduledTimer(withTimeInterval: interval, repeats: repeats) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                action(self)
            }
        }
    }

    private func render() {
        guard let host else { return }
        switch phase {
        case .idle:
            renderIdle(host)
        case .opening:
            host.showStatus("Opening Simple Voice…", tone: .working)
            host.showMic(.idle)
        case .starting:
            host.showStatus("Starting…", tone: .working)
            host.showMic(.busy)
        case .stalled:
            host.showStatus("Simple Voice didn't answer · tap the mic to retry", tone: .blocked)
            host.showMic(.idle)
        case .recording(let since):
            let seconds = max(Int(Date().timeIntervalSince(since)), 0)
            // The watchdog stops and transcribes a recording left running (D-13).
            if seconds >= maxSeconds {
                stop()
                return
            }
            host.showStatus(String(format: "Recording · %d:%02d", seconds / 60, seconds % 60), tone: .recording)
            host.showMic(.recording)
        case .working(let line):
            host.showStatus(line, tone: .working)
            host.showMic(.busy)
        case .notice(let line, let tone):
            host.showStatus(line, tone: tone)
            host.showMic(idleMic)
        }
    }

    private func renderIdle(_ host: DictationHost) {
        guard host.hasFullAccess, files != nil else {
            host.showStatus("Allow Full Access for Simple Voice", tone: .blocked)
            host.showMic(.disabled)
            return
        }
        if retryId != nil {
            host.showStatus("Not sent · tap to retry", tone: .retry)
        } else if let status {
            host.showStatus(status.line, tone: status.ready ? .ready : .blocked)
        } else {
            host.showStatus("Open Simple Voice to set it up", tone: .blocked)
        }
        host.showMic(idleMic)
    }

    private var idleMic: MicState {
        guard let host, host.hasFullAccess, files != nil, let status, status.ready, !status.needsKey
        else { return .disabled }
        return .idle
    }

    private var maxSeconds: Int {
        guard let seconds = status?.maxSeconds, seconds > 0 else { return Self.defaultMaxSeconds }
        return seconds
    }
}
