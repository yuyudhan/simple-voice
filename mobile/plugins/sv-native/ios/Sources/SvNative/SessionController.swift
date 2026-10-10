// FilePath: mobile/plugins/sv-native/ios/Sources/SvNative/SessionController.swift
// The iPhone session (MB-7): the keyboard cannot use the microphone, so the app records for it.
// The first mic tap opens the app, which starts the session; the session keeps the microphone
// running in the background for the session length, and later taps never leave the keyboard.
// The keyboard and the app talk through App Group files and Darwin notifications (SvShared).
// Main thread only.

import AVFoundation
import Foundation
import Network
import SvShared
import UIKit

final class SessionController {
    let core: RustCore
    private let files = SharedFiles()
    private let recorder = Recorder()
    private let activity = LiveActivity()
    private let speech = AppleSpeech()
    private let network = NWPathMonitor()

    private var online = true
    private var status: NativeStatus?
    /// Overrides the status line while the session cannot start (microphone refused).
    private var notice: String?
    private var endsAt: Date?
    /// The keyboard asked for a session while the app was still coming to the foreground.
    private var sessionWanted = false
    private var recordingRequest: String?
    private var recordingSince: Date?
    private var heartbeat: Timer?
    private var meter: Timer?
    /// Text replies waiting for the keyboard's `ack`, by history id.
    private var unacknowledged: [Int64: Timer] = [:]

    private static let heartbeatSeconds: TimeInterval = 5
    private static let ackSeconds: TimeInterval = 2
    private static let meterSeconds: TimeInterval = 0.08

    init(core: RustCore) {
        self.core = core
    }

    func start() {
        DarwinNote.observe(Bridge.Note.request) { [weak self] in self?.handleRequests() }
        DarwinNote.observe(Bridge.Note.endSession) { [weak self] in self?.endSession() }
        let center = NotificationCenter.default
        center.addObserver(forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in
            guard let self else { return }
            self.handleRequests()
            if self.sessionWanted { self.startSession() }
        }
        center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] note in
            let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt
            if raw == AVAudioSession.InterruptionType.began.rawValue { self?.endSession() }
        }
        network.pathUpdateHandler = { [weak self] path in
            DispatchQueue.main.async {
                guard let self else { return }
                let online = path.status == .satisfied
                guard online != self.online || self.status == nil else { return }
                self.online = online
                self.refreshStatus()
            }
        }
        network.start(queue: DispatchQueue(label: "dev.yuyudhan.simplevoice.network"))
        refreshStatus()
        handleRequests()
    }

    /// Asks Rust for the status line, then rewrites the status file.
    func refreshStatus() {
        speech.refresh(online: online) { [weak self] in
            guard let self else { return }
            let ask = NativeAsk.status(online: self.online, onDeviceSpeech: self.speech.isAvailable)
            self.core.ask(ask, as: NativeStatus.self) { [weak self] status in
                guard let self else { return }
                if let status { self.status = status }
                self.publish()
            }
        }
    }

    // MARK: Keyboard requests

    private func handleRequests() {
        guard let files else { return }
        for request in files.takeRequests() {
            switch request.kind {
            case .startSession: startSession()
            case .start: startRecording(request)
            case .stop: stopRecording(request)
            case .cancel: cancelRecording()
            case .retry: retry(request)
            case .ack: if let id = request.historyId { unacknowledged.removeValue(forKey: id)?.invalidate() }
            }
        }
    }

    private func startSession() {
        if endsAt != nil {
            extendSession()
            return
        }
        // The engine may only start in the foreground. The keyboard posts its request just before
        // opening the app, so it usually arrives while the app is still in the background.
        guard UIApplication.shared.applicationState == .active else {
            sessionWanted = true
            return
        }
        sessionWanted = false
        AVAudioApplication.requestRecordPermission { [weak self] granted in
            DispatchQueue.main.async { self?.begin(granted: granted) }
        }
    }

    private func begin(granted: Bool) {
        guard granted else {
            notice = "Allow the microphone for Simple Voice in Settings"
            publish()
            return
        }
        do {
            try recorder.start()
        } catch {
            notice = error.localizedDescription
            publish()
            return
        }
        notice = nil
        extendSession()
        heartbeat?.invalidate()
        heartbeat = Timer.scheduledTimer(withTimeInterval: Self.heartbeatSeconds, repeats: true) { [weak self] _ in
            self?.tick()
        }
    }

    private func startRecording(_ request: KeyboardRequest) {
        guard endsAt != nil, recorder.running else {
            reply(request.id, .error, message: "Open Simple Voice to start a session")
            return
        }
        if recordingRequest != nil { recorder.discard() }
        let name = "session-\(Int64(Date().timeIntervalSince1970 * 1000)).wav"
        do {
            try recorder.begin(to: Self.audioDirectory.appendingPathComponent(name))
        } catch {
            reply(request.id, .error, message: error.localizedDescription)
            return
        }
        recordingRequest = request.id
        recordingSince = Date()
        extendSession()
        meter?.invalidate()
        meter = Timer.scheduledTimer(withTimeInterval: Self.meterSeconds, repeats: true) { [weak self] _ in
            guard let self else { return }
            self.files?.level = self.recorder.level
        }
        reply(request.id, .recording)
    }

    private func stopRecording(_ request: KeyboardRequest) {
        guard let recording = endRecording() else {
            reply(request.id, .error, message: "Nothing was recording")
            return
        }
        let path = recording.url.path
        if online {
            core.ask(.dictate(wavPath: path, durationMs: recording.durationMs), as: Dictation.self) { [weak self] answer in
                self?.deliver(answer, to: request.id)
            }
        } else if speech.isAvailable, status?.ready == true {
            speech.transcribe(recording.url) { [weak self] result in
                guard let self else { return }
                switch result {
                case let .success(heard):
                    let ask = NativeAsk.local(
                        wavPath: path, durationMs: recording.durationMs, text: heard.text,
                        language: heard.language, transcribeMs: heard.transcribeMs
                    )
                    self.core.ask(ask, as: Dictation.self) { [weak self] answer in self?.deliver(answer, to: request.id) }
                case let .failure(error):
                    try? FileManager.default.removeItem(at: recording.url)
                    self.reply(request.id, .error, message: error.localizedDescription)
                }
            }
        } else {
            try? FileManager.default.removeItem(at: recording.url)
            reply(request.id, .error, message: "No internet · can't dictate")
        }
    }

    private func cancelRecording() {
        guard recordingRequest != nil else { return }
        recorder.discard()
        _ = endRecording()
    }

    private func retry(_ request: KeyboardRequest) {
        guard let id = request.historyId else { return }
        core.ask(.retry(historyId: id), as: Dictation.self) { [weak self] answer in
            self?.deliver(answer, to: request.id)
        }
    }

    // MARK: Results

    /// Stops writing; the file and its length, or nil when nothing was recording.
    private func endRecording() -> (url: URL, durationMs: Int64)? {
        meter?.invalidate()
        meter = nil
        files?.level = 0
        recordingRequest = nil
        recordingSince = nil
        let recording = recorder.finish()
        publish()
        return recording
    }

    private func deliver(_ answer: Dictation?, to requestId: String) {
        guard let answer else {
            reply(requestId, .error, message: "Simple Voice could not read the result")
            return
        }
        switch answer.kind {
        case "text":
            let text = answer.text ?? ""
            reply(requestId, .text, text: text, historyId: answer.id)
            awaitAck(text: text, historyId: answer.id ?? 0)
        case "noSpeech":
            reply(requestId, .noSpeech)
        default:
            reply(requestId, .failed, historyId: answer.id, message: answer.message)
        }
    }

    /// No `ack` means no keyboard took the text (the field closed): copy it instead (D-15).
    private func awaitAck(text: String, historyId: Int64) {
        unacknowledged[historyId]?.invalidate()
        unacknowledged[historyId] = Timer.scheduledTimer(withTimeInterval: Self.ackSeconds, repeats: false) { [weak self] _ in
            guard let self else { return }
            self.unacknowledged.removeValue(forKey: historyId)
            UIPasteboard.general.string = text.trimmingCharacters(in: .whitespacesAndNewlines)
            if historyId > 0 {
                self.core.ask(.notInserted(historyId: historyId), as: Bool.self) { _ in }
            }
        }
    }

    private func reply(_ requestId: String, _ kind: ReplyKind, text: String? = nil, historyId: Int64? = nil, message: String? = nil) {
        files?.writeReply(KeyboardReply(requestId: requestId, kind: kind, text: text, historyId: historyId, message: message))
    }

    // MARK: Session

    private func extendSession() {
        let minutes = TimeInterval(status?.sessionMinutes ?? 15)
        endsAt = Date().addingTimeInterval(minutes * 60)
        publish()
    }

    private func tick() {
        guard let endsAt else { return }
        if Date() >= endsAt, recordingRequest == nil {
            endSession()
        } else {
            publish()
        }
    }

    func endSession() {
        if let request = recordingRequest {
            recorder.discard()
            _ = endRecording()
            reply(request, .sessionEnded)
        }
        recorder.stop()
        heartbeat?.invalidate()
        heartbeat = nil
        endsAt = nil
        activity.end()
        publish()
    }

    /// Writes the status file and updates the Live Activity.
    private func publish() {
        let now = Date()
        let line = notice ?? status?.line ?? "Starting Simple Voice…"
        let ms = { (date: Date) in date.timeIntervalSince1970 * 1000 }
        let shared = SharedStatus(
            line: line,
            ready: notice == nil && (status?.ready ?? false),
            needsKey: status?.needsKey ?? false,
            maxSeconds: status?.maxSeconds ?? 300,
            session: endsAt.map { SharedStatus.Session(endsAt: ms($0), aliveAt: ms(now)) },
            recording: recordingSince.map { SharedStatus.Recording(startedAt: ms($0)) },
            updatedAt: ms(now)
        )
        files?.writeStatus(shared)
        if let endsAt {
            let model = line.components(separatedBy: " · ").first ?? line
            activity.show(endsAt: endsAt, recordingSince: recordingSince, model: model)
        }
    }

    private static var audioDirectory: URL {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
        let base = support.first ?? FileManager.default.temporaryDirectory
        return base.appendingPathComponent("audio", isDirectory: true)
    }
}
