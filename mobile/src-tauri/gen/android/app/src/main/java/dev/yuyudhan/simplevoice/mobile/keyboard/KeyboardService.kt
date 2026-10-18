// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/KeyboardService.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.content.ClipData
import android.content.ClipboardManager
import android.content.pm.PackageManager
import android.inputmethodservice.InputMethodService
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import org.json.JSONObject
import java.io.File
import java.util.concurrent.Executors
import kotlin.math.abs

/**
 * The Simple Voice keyboard: a full keyboard for typing and fixes, and a mic in its top bar
 * that records, sends the audio through the Rust core (Groq, formatting, history) and inserts
 * the text at the cursor; with text selected the recording edits the selection instead
 * ([EditMode]). Network calls run on [worker]; the views change on the main thread.
 */
class KeyboardService : InputMethodService() {
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor()
    private var view: KeyboardView? = null
    private var coreError = ""
    private var layer = Layer.LETTERS
    private var shift = ShiftState.OFF
    private var lastShiftTap = 0L
    private lateinit var suggestions: Suggestions

    /** The last key typed a space, so a second one makes a full stop (". "). */
    private var lastWasSpace = false

    /** The space before the cursor came from a suggestion or dictation, not the space bar. */
    private var autoSpace = false

    private var recorder: WavRecorder? = null
    private var recordingFile: File? = null
    private var recordingStarted = 0L
    private var busy = false
    private var ready = false
    private var fix: Fix? = null
    private var maxSeconds = KeyboardStatus.DEFAULT_MAX_SECONDS
    private var editMaxChars = 0
    private lateinit var keyboardStatus: KeyboardStatus
    private var retry: Retry? = null

    /** The edit being recorded or sent; null while the mic dictates. */
    private var edit: EditRequest? = null

    /** The selection's length as the field last reported it; above 0 the mic edits. */
    private var selectionLength = 0

    private val listener = object : KeyboardListener {
        override fun onKey(key: Key) = handleKey(key)
        override fun onMic() = handleMic()
        override fun onStatusTap() = handleStatusTap()
        override fun onSuggestion(word: String) = handleSuggestion(word)
        override fun onCursor(offset: Int) = moveCursor(offset)
        override fun onCapsLock() {
            shift = ShiftState.LOCKED
            view?.setLayer(layer, shift)
        }
    }

    private val tick = object : Runnable {
        override fun run() {
            if (recorder == null) return
            val seconds = (SystemClock.elapsedRealtime() - recordingStarted) / 1000
            val label = if (edit != null) "Editing" else "Recording"
            view?.setStatus("%s · %d:%02d".format(label, seconds / 60, seconds % 60), Tone.RECORDING)
            // The watchdog stops and transcribes a recording left running (D-13).
            if (seconds >= maxSeconds) stopAndDictate() else main.postDelayed(this, 1000)
        }
    }

    private val meter = object : Runnable {
        override fun run() {
            val active = recorder ?: return
            view?.setLevel(active.level)
            main.postDelayed(this, METER_MS)
        }
    }

    override fun onCreate() {
        super.onCreate()
        coreError = SvCore.init(dataDir.absolutePath)
        EditMode.sweep(filesDir)
        keyboardStatus = KeyboardStatus(this) { main.post { refreshStatus() } }
        keyboardStatus.start()
        suggestions = Suggestions(this) { view?.setSuggestions(it) }
    }

    override fun onDestroy() {
        keyboardStatus.stop()
        suggestions.close()
        recorder?.cancel()
        worker.shutdown()
        super.onDestroy()
    }

    override fun onCreateInputView(): View {
        val keyboard = KeyboardView(this, Palette.of(resources.configuration), listener)
        view = keyboard
        keyboard.setEditing(selectionLength > 0)
        refreshStatus()
        return keyboard
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        if (!restarting) layer = Layer.LETTERS
        lastWasSpace = false
        autoSpace = false
        val start = info?.initialSelStart ?: -1
        val end = info?.initialSelEnd ?: -1
        trackSelection(if (start < 0 || end < 0) 0 else abs(end - start))
        view?.setReturnAction(ReturnAction.of(info))
        updateShiftFromCursor()
        updateSuggestions()
        refreshStatus()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        // Leaving the field mid-recording abandons the recording, like Esc on the Mac (D-3).
        main.removeCallbacks(tick)
        main.removeCallbacks(meter)
        if (recorder != null) edit = null
        recorder?.cancel()
        recorder = null
        super.onFinishInputView(finishingInput)
    }

    override fun onUpdateSelection(
        oldSelStart: Int,
        oldSelEnd: Int,
        newSelStart: Int,
        newSelEnd: Int,
        candidatesStart: Int,
        candidatesEnd: Int,
    ) {
        super.onUpdateSelection(oldSelStart, oldSelEnd, newSelStart, newSelEnd, candidatesStart, candidatesEnd)
        trackSelection(abs(newSelEnd - newSelStart))
        updateShiftFromCursor()
        updateSuggestions()
    }

    private fun trackSelection(length: Int) {
        selectionLength = length
        view?.setEditing(length > 0)
    }

    private fun handleKey(key: Key) {
        val input = currentInputConnection ?: return
        val wasSpace = lastWasSpace
        val wasAutoSpace = autoSpace
        lastWasSpace = false
        autoSpace = false
        when (key) {
            is Key.Text -> {
                val text = if (shift != ShiftState.OFF) key.text.uppercase() else key.text
                // "word ," → "word, ": punctuation joins the word a suggestion or dictation ended.
                if (wasAutoSpace && text in CLOSING_PUNCTUATION && spaceBeforeCursor()) {
                    input.deleteSurroundingText(1, 0)
                    input.commitText("$text ", 1)
                    autoSpace = true
                } else {
                    input.commitText(text, 1)
                }
                if (shift == ShiftState.ONCE) shift = ShiftState.OFF
            }
            Key.Shift -> {
                val now = SystemClock.uptimeMillis()
                shift = when {
                    shift == ShiftState.ONCE && now - lastShiftTap < DOUBLE_TAP_MS -> ShiftState.LOCKED
                    shift == ShiftState.OFF -> ShiftState.ONCE
                    else -> ShiftState.OFF
                }
                lastShiftTap = now
            }
            Key.Backspace -> sendDownUpKeyEvents(KeyEvent.KEYCODE_DEL)
            Key.Space -> typeSpace(wasSpace)
            Key.Return -> if (!sendDefaultEditorAction(true)) sendKeyChar('\n')
            Key.NextKeyboard -> switchKeyboard()
            is Key.SwitchLayer -> {
                layer = key.layer
                shift = ShiftState.OFF
                if (layer == Layer.LETTERS) updateShiftFromCursor()
            }
        }
        // Symbols are typed in runs; a space after them goes back to the letters, as on iOS.
        if (key == Key.Space && layer != Layer.LETTERS) {
            layer = Layer.LETTERS
            updateShiftFromCursor()
        }
        view?.setLayer(layer, shift)
    }

    /** A second space right after a word ends the sentence: "word  " becomes "word. ". */
    private fun typeSpace(afterSpace: Boolean) {
        val input = currentInputConnection ?: return
        val before = input.getTextBeforeCursor(2, 0)?.toString() ?: ""
        if (afterSpace && before.length == 2 && before[0].isLetterOrDigit() && before[1] == ' ') {
            input.deleteSurroundingText(1, 0)
            input.commitText(". ", 1)
            return
        }
        input.commitText(" ", 1)
        lastWasSpace = true
    }

    private fun spaceBeforeCursor(): Boolean =
        currentInputConnection?.getTextBeforeCursor(1, 0)?.toString() == " "

    private fun moveCursor(offset: Int) {
        val code = if (offset < 0) KeyEvent.KEYCODE_DPAD_LEFT else KeyEvent.KEYCODE_DPAD_RIGHT
        repeat(abs(offset)) { sendDownUpKeyEvents(code) }
        lastWasSpace = false
        autoSpace = false
    }

    private fun handleMic() {
        if (busy) return
        if (recorder != null) {
            stopAndDictate()
            return
        }
        if (!ready || !keyboardStatus.microphoneAllowed()) {
            // The key may have been added, the microphone allowed (or taken back: "Only this
            // time" lapses) or the connection come back since the keyboard last looked; check
            // again. A blocked mic never leaves the keyboard: the status line says what to tap.
            // Like a new recording, this replaces a waiting Retry; history keeps the row.
            dropRetry()
            refreshStatus { if (ready) startRecording() else view?.flashStatus() }
            return
        }
        startRecording()
    }

    private fun startRecording() {
        // Checked at the tap as well: some apps report a selection late or never.
        val selected = currentInputConnection?.getSelectedText(0)?.toString()
        val editing = selectionLength > 0 || !selected.isNullOrEmpty()
        if (editing) {
            val problem = EditMode.problem(selected, selectionLength, editMaxChars)
            if (problem != null) {
                dropRetry()
                notice(problem, Tone.BLOCKED)
                return
            }
        }
        val file = if (editing) {
            EditMode.file(filesDir)
        } else {
            File(filesDir, "audio/session-${System.currentTimeMillis()}.wav")
        }
        val next = WavRecorder(file)
        if (!next.start()) {
            view?.setStatus("The microphone is busy or unavailable", Tone.BLOCKED)
            return
        }
        recorder = next
        recordingFile = file
        recordingStarted = SystemClock.elapsedRealtime()
        dropRetry()
        edit = selected?.takeIf { editing }?.let { EditRequest(it, file) }
        view?.setMic(MicState.RECORDING)
        tick.run()
        meter.run()
    }

    /** A new recording or a blocked mic replaces a waiting Retry; history keeps its row. */
    private fun dropRetry() {
        (retry as? Retry.Edit)?.request?.file?.delete()
        retry = null
    }

    private fun handleStatusTap() {
        val pending = retry
        when {
            busy || recorder != null -> return
            pending != null -> retry(pending)
            else -> fix?.let { keyboardStatus.openApp(it) }
        }
    }

    private fun handleSuggestion(word: String) {
        val input = currentInputConnection ?: return
        input.deleteSurroundingText(suggestions.word.length, 0)
        input.commitText("$word ", 1)
        autoSpace = true
        lastWasSpace = false
    }

    private fun stopAndDictate() {
        main.removeCallbacks(tick)
        main.removeCallbacks(meter)
        val active = recorder ?: return
        val file = recordingFile ?: return
        val pending = edit
        recorder = null
        busy = true
        view?.setMic(MicState.BUSY)
        view?.setStatus(if (pending != null) "Editing…" else "Transcribing…", Tone.WORKING)
        val appId = currentInputEditorInfo?.packageName
        val appName = appId?.let { label(it) }
        worker.execute {
            val durationMs = active.stop()
            val request = pending?.copy(durationMs = durationMs, appName = appName, appId = appId)
            val answer = if (request != null) {
                EditMode.send(request)
            } else {
                SvCore.dictate(file.absolutePath, durationMs, appName, appId)
            }
            main.post {
                busy = false
                deliver(JSONObject(answer), request)
            }
        }
    }

    private fun retry(pending: Retry) {
        busy = true
        view?.setMic(MicState.BUSY)
        view?.setStatus("Retrying…", Tone.WORKING)
        val request = (pending as? Retry.Edit)?.request
        worker.execute {
            val answer = when (pending) {
                is Retry.Dictation -> SvCore.retry(pending.id)
                is Retry.Edit -> EditMode.send(pending.request)
            }
            main.post {
                busy = false
                // The core now owns the audio: it kept it again only if it answers with an id.
                retry = null
                deliver(JSONObject(answer), request)
            }
        }
    }

    /**
     * Inserts a dictation's text or replaces [request]'s selection with an edit's, or says why
     * there is none (MB-5: never without a tap).
     */
    private fun deliver(answer: JSONObject, request: EditRequest?) {
        edit = null
        when (answer.optString("kind")) {
            "text" -> {
                val text = answer.getString("text")
                val id = answer.optLong("id")
                if (request == null) {
                    insert(text, id)
                    refreshStatus()
                } else {
                    replaceSelection(request, text, id)
                }
            }
            "noSpeech" -> notice("Nothing heard", Tone.WORKING)
            else -> {
                val id = answer.optLong("id", 0)
                if (id > 0) {
                    retry = request?.let { Retry.Edit(it.copy(historyId = id)) } ?: Retry.Dictation(id)
                    view?.setStatus("Not sent · tap to retry", Tone.RETRY)
                    view?.setMic(if (ready) MicState.IDLE else MicState.DISABLED)
                } else {
                    notice(answer.optString("message", "Dictation failed"), Tone.BLOCKED)
                }
            }
        }
    }

    override fun onWindowShown() {
        super.onWindowShown()
        // Back from the app (a new Groq key, the microphone allowed): show the current state.
        refreshStatus()
    }

    /** A field that went away while Groq worked gets the clipboard instead (D-15). */
    private fun insert(text: String, id: Long) {
        // Dictations never run into the word before the cursor (D-16 covers the word after).
        val before = currentInputConnection?.getTextBeforeCursor(1, 0)?.toString() ?: ""
        val joins = before.isNotEmpty() && !before.last().isWhitespace()
        if (currentInputConnection?.commitText(if (joins) " $text" else text, 1) == true) {
            // Dictated text ends with a space (D-16); a comma typed next joins the last word.
            autoSpace = text.endsWith(" ")
            lastWasSpace = false
            return
        }
        copyInstead(text.trimEnd(), id, COPIED_FIELD_CLOSED, FIELD_CLOSED)
    }

    /** An edit lands only on the selection it was made from; otherwise it is copied. */
    private fun replaceSelection(request: EditRequest, text: String, id: Long) {
        val input = currentInputConnection
        val current = input?.getSelectedText(0)?.toString()
        if (input != null && current == request.selection && input.commitText(text, 1)) {
            autoSpace = false
            lastWasSpace = false
            notice(EditMode.EDITED, Tone.WORKING)
        } else if (input?.getTextBeforeCursor(1, 0) == null) {
            // A live field answers even with nothing selected; a closed one answers nothing.
            copyInstead(text, id, COPIED_FIELD_CLOSED, FIELD_CLOSED)
        } else {
            copyInstead(text, id, EditMode.SELECTION_CHANGED, EditMode.SELECTION_CHANGED_REASON)
        }
    }

    private fun copyInstead(text: String, id: Long, line: String, reason: String) {
        getSystemService(ClipboardManager::class.java)
            ?.setPrimaryClip(ClipData.newPlainText("Simple Voice", text))
        if (id > 0) worker.execute { SvCore.markNotInserted(id, reason) }
        notice(line, Tone.WORKING)
    }

    /** Shows [line] for a moment with the mic usable again, then the usual status. */
    private fun notice(line: String, tone: Tone) {
        view?.setStatus(line, tone)
        view?.setMic(if (ready) MicState.IDLE else MicState.DISABLED)
        main.postDelayed({ refreshStatus() }, NOTICE_MS)
    }

    /** Reads what the top bar should say, then runs [then] with the new state. */
    private fun refreshStatus(then: (() -> Unit)? = null) {
        if (recorder != null || busy || retry != null) return
        if (coreError.isNotEmpty()) {
            view?.setStatus(coreError, Tone.BLOCKED)
            view?.setMic(MicState.DISABLED)
            return
        }
        worker.execute {
            val status = keyboardStatus.read()
            main.post {
                if (recorder != null || busy || retry != null) return@post
                ready = status.ready
                fix = status.fix
                maxSeconds = status.maxSeconds
                editMaxChars = status.editMaxChars
                view?.setStatus(status.line, if (ready) Tone.READY else Tone.BLOCKED)
                view?.setMic(if (ready) MicState.IDLE else MicState.DISABLED)
                then?.invoke()
            }
        }
    }

    /** Capitalises the next letter where the field asks for it (sentence starts, names). */
    private fun updateShiftFromCursor() {
        if (layer != Layer.LETTERS || shift == ShiftState.LOCKED) return
        val info = currentInputEditorInfo ?: return
        val caps = currentInputConnection?.getCursorCapsMode(info.inputType) ?: 0
        shift = if (caps != 0) ShiftState.ONCE else ShiftState.OFF
        view?.setLayer(layer, shift)
    }

    /** Asks the system spell checker about the word being typed. */
    private fun updateSuggestions() = suggestions.update(
        currentInputConnection?.getTextBeforeCursor(Suggestions.LOOKBACK, 0)?.toString() ?: "",
    )

    private fun switchKeyboard() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            switchToNextInputMethod(false)
        } else {
            requestHideSelf(0)
        }
    }

    /** The app's name for history, when Android lets the keyboard see it. */
    private fun label(packageName: String): String? = try {
        packageManager.getApplicationLabel(packageManager.getApplicationInfo(packageName, 0)).toString()
    } catch (error: PackageManager.NameNotFoundException) {
        null
    }

    companion object {
        private const val DOUBLE_TAP_MS = 300L
        private const val NOTICE_MS = 2500L
        private const val METER_MS = 80L
        private const val COPIED_FIELD_CLOSED = "Copied · the text field closed"
        private const val FIELD_CLOSED = "The text field closed first; the text was copied"
        private val CLOSING_PUNCTUATION = setOf(".", ",", "?", "!", ":", ";")
    }
}
