// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/KeyboardService.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.PackageManager
import android.inputmethodservice.InputMethodService
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.textservice.SentenceSuggestionsInfo
import android.view.textservice.SpellCheckerSession
import android.view.textservice.SuggestionsInfo
import android.view.textservice.TextInfo
import android.view.textservice.TextServicesManager
import androidx.core.view.WindowInsetsCompat
import dev.yuyudhan.simplevoice.mobile.MainActivity
import org.json.JSONObject
import java.io.File
import java.util.concurrent.Executors
import kotlin.math.abs

/**
 * The Simple Voice keyboard: a full keyboard for typing and fixes, and a mic in its top bar
 * that records, sends the audio through the Rust core (Groq, formatting, history) and inserts
 * the text at the cursor. Network calls run on [worker]; the views change on the main thread.
 */
class KeyboardService :
    InputMethodService(),
    SpellCheckerSession.SpellCheckerSessionListener {
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor()
    private var view: KeyboardView? = null
    private var coreError = ""
    private var layer = Layer.LETTERS
    private var shift = ShiftState.OFF
    private var lastShiftTap = 0L
    private var spell: SpellCheckerSession? = null
    private var currentWord = ""

    /** The last key typed a space, so a second one makes a full stop (". "). */
    private var lastWasSpace = false

    /** The space before the cursor came from a suggestion or dictation, not the space bar. */
    private var autoSpace = false

    private var recorder: WavRecorder? = null
    private var recordingFile: File? = null
    private var recordingStarted = 0L
    private var busy = false
    private var ready = false
    private var keyMissing = false
    private var maxSeconds = DEFAULT_MAX_SECONDS
    private var retryId: Long? = null

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

    private val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
            main.post { refreshStatus() }
        }

        override fun onLost(network: Network) {
            main.post { refreshStatus() }
        }
    }

    private val tick = object : Runnable {
        override fun run() {
            if (recorder == null) return
            val seconds = (SystemClock.elapsedRealtime() - recordingStarted) / 1000
            view?.setStatus("Recording · %d:%02d".format(seconds / 60, seconds % 60), Tone.RECORDING)
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
        connectivity().registerDefaultNetworkCallback(networkCallback)
        spell = getSystemService(TextServicesManager::class.java)
            ?.newSpellCheckerSession(null, null, this, true)
    }

    override fun onDestroy() {
        connectivity().unregisterNetworkCallback(networkCallback)
        spell?.close()
        recorder?.cancel()
        worker.shutdown()
        super.onDestroy()
    }

    override fun onCreateInputView(): View {
        val keyboard = KeyboardView(this, Palette.of(resources.configuration), listener)
        // Android draws the keyboard behind the navigation bar; keep the keys above it.
        keyboard.setOnApplyWindowInsetsListener { target, insets ->
            val bottom = WindowInsetsCompat.toWindowInsetsCompat(insets, target)
                .getInsets(WindowInsetsCompat.Type.navigationBars())
                .bottom
            target.setPadding(target.paddingLeft, target.paddingTop, target.paddingRight, bottom)
            insets
        }
        view = keyboard
        refreshStatus()
        return keyboard
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        if (!restarting) layer = Layer.LETTERS
        lastWasSpace = false
        autoSpace = false
        view?.setReturnAction(returnAction(info))
        updateShiftFromCursor()
        updateSuggestions()
        refreshStatus()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        // Leaving the field mid-recording abandons the recording, like Esc on the Mac (D-3).
        main.removeCallbacks(tick)
        main.removeCallbacks(meter)
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
        updateShiftFromCursor()
        updateSuggestions()
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
        if (!ready) {
            // The key may have been added, or the connection come back, since the keyboard last
            // looked; check again before saying no.
            refreshStatus {
                when {
                    ready -> handleMic()
                    keyMissing -> openApp(requestMicrophone = false)
                }
            }
            return
        }
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            view?.setStatus("Allow the microphone in Simple Voice", Tone.BLOCKED)
            openApp(requestMicrophone = true)
            return
        }
        val file = File(filesDir, "audio/session-${System.currentTimeMillis()}.wav")
        val next = WavRecorder(file)
        if (!next.start()) {
            view?.setStatus("The microphone is busy or unavailable", Tone.BLOCKED)
            return
        }
        recorder = next
        recordingFile = file
        recordingStarted = SystemClock.elapsedRealtime()
        retryId = null
        view?.setMic(MicState.RECORDING)
        tick.run()
        meter.run()
    }

    private fun handleStatusTap() {
        val id = retryId
        when {
            busy || recorder != null -> return
            id != null -> retry(id)
            keyMissing -> openApp(requestMicrophone = false)
        }
    }

    private fun handleSuggestion(word: String) {
        val input = currentInputConnection ?: return
        input.deleteSurroundingText(currentWord.length, 0)
        input.commitText("$word ", 1)
        autoSpace = true
        lastWasSpace = false
    }

    override fun onGetSuggestions(results: Array<out SuggestionsInfo>?) = Unit

    override fun onGetSentenceSuggestions(results: Array<out SentenceSuggestionsInfo>?) {
        val words = mutableListOf<String>()
        val sentence = results?.firstOrNull()
        if (sentence != null && sentence.suggestionsCount > 0) {
            val info = sentence.getSuggestionsInfoAt(0)
            for (index in 0 until info.suggestionsCount) words.add(info.getSuggestionAt(index))
        }
        main.post { view?.setSuggestions(words) }
    }

    private fun stopAndDictate() {
        main.removeCallbacks(tick)
        main.removeCallbacks(meter)
        val active = recorder ?: return
        val file = recordingFile ?: return
        recorder = null
        busy = true
        view?.setMic(MicState.BUSY)
        view?.setStatus("Transcribing…", Tone.WORKING)
        val appId = currentInputEditorInfo?.packageName
        val appName = appId?.let { label(it) }
        worker.execute {
            val durationMs = active.stop()
            val answer = SvCore.dictate(file.absolutePath, durationMs, appName, appId)
            main.post {
                busy = false
                deliver(JSONObject(answer))
            }
        }
    }

    private fun retry(id: Long) {
        busy = true
        view?.setMic(MicState.BUSY)
        view?.setStatus("Retrying…", Tone.WORKING)
        worker.execute {
            val answer = SvCore.retry(id)
            main.post {
                busy = false
                retryId = null
                deliver(JSONObject(answer))
            }
        }
    }

    /** Inserts a dictation's text, or says why there is none (MB-5: never without a tap). */
    private fun deliver(answer: JSONObject) {
        when (answer.optString("kind")) {
            "text" -> {
                insert(answer.getString("text"), answer.optLong("id"))
                refreshStatus()
            }
            "noSpeech" -> {
                view?.setStatus("Nothing heard", Tone.WORKING)
                view?.setMic(MicState.IDLE)
                main.postDelayed({ refreshStatus() }, NOTICE_MS)
            }
            else -> {
                val id = answer.optLong("id", 0)
                if (id > 0) {
                    retryId = id
                    view?.setStatus("Not sent · tap to retry", Tone.RETRY)
                    view?.setMic(if (ready) MicState.IDLE else MicState.DISABLED)
                } else {
                    view?.setStatus(answer.optString("message", "Dictation failed"), Tone.BLOCKED)
                    main.postDelayed({ refreshStatus() }, NOTICE_MS)
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
        if (currentInputConnection?.commitText(text, 1) == true) {
            // Dictated text ends with a space (D-16); a comma typed next joins the last word.
            autoSpace = text.endsWith(" ")
            lastWasSpace = false
            return
        }
        getSystemService(ClipboardManager::class.java)
            ?.setPrimaryClip(ClipData.newPlainText("Simple Voice", text.trimEnd()))
        if (id > 0) {
            worker.execute { SvCore.markNotInserted(id, "The text field closed first; the text was copied") }
        }
        view?.setStatus("Copied · the text field closed", Tone.WORKING)
        main.postDelayed({ refreshStatus() }, NOTICE_MS)
    }

    /** Asks the Rust core what the top bar should say, then runs [then] with the new state. */
    private fun refreshStatus(then: (() -> Unit)? = null) {
        if (recorder != null || busy || retryId != null) return
        if (coreError.isNotEmpty()) {
            view?.setStatus(coreError, Tone.BLOCKED)
            view?.setMic(MicState.DISABLED)
            return
        }
        val online = isOnline()
        worker.execute {
            val status = JSONObject(SvCore.status(online))
            main.post {
                if (recorder != null || busy || retryId != null) return@post
                ready = status.optBoolean("ready")
                keyMissing = status.optBoolean("needsKey")
                maxSeconds = status.optInt("maxSeconds", DEFAULT_MAX_SECONDS).takeIf { it > 0 }
                    ?: DEFAULT_MAX_SECONDS
                view?.setStatus(status.optString("line"), if (ready) Tone.READY else Tone.BLOCKED)
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
    private fun updateSuggestions() {
        val before = currentInputConnection?.getTextBeforeCursor(WORD_LOOKBACK, 0)?.toString() ?: ""
        currentWord = before.takeLastWhile { it.isLetter() || it == '\'' }
        val session = spell
        if (currentWord.length < 2 || session == null) {
            view?.setSuggestions(emptyList())
            return
        }
        session.getSentenceSuggestions(arrayOf(TextInfo(currentWord)), SUGGESTIONS)
    }

    private fun returnAction(info: EditorInfo?): ReturnAction {
        val options = info?.imeOptions ?: return ReturnAction.NEWLINE
        if (options and EditorInfo.IME_FLAG_NO_ENTER_ACTION != 0) return ReturnAction.NEWLINE
        return when (options and EditorInfo.IME_MASK_ACTION) {
            EditorInfo.IME_ACTION_SEND -> ReturnAction.SEND
            EditorInfo.IME_ACTION_SEARCH -> ReturnAction.SEARCH
            EditorInfo.IME_ACTION_GO -> ReturnAction.GO
            EditorInfo.IME_ACTION_NEXT -> ReturnAction.NEXT
            EditorInfo.IME_ACTION_DONE -> ReturnAction.DONE
            else -> ReturnAction.NEWLINE
        }
    }

    private fun switchKeyboard() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            switchToNextInputMethod(false)
        } else {
            requestHideSelf(0)
        }
    }

    /** Opens the app: to enter the Groq key, or to ask for the microphone. */
    private fun openApp(requestMicrophone: Boolean) {
        val intent = Intent(this, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            .putExtra(EXTRA_REQUEST_MICROPHONE, requestMicrophone)
        startActivity(intent)
    }

    /** The app's name for history, when Android lets the keyboard see it. */
    private fun label(packageName: String): String? = try {
        packageManager.getApplicationLabel(packageManager.getApplicationInfo(packageName, 0)).toString()
    } catch (error: PackageManager.NameNotFoundException) {
        null
    }

    /** Online means a validated connection, not just Wi-Fi joined behind a login page. */
    private fun isOnline(): Boolean {
        val manager = connectivity()
        val capabilities = manager.getNetworkCapabilities(manager.activeNetwork) ?: return false
        return capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
    }

    private fun connectivity(): ConnectivityManager = getSystemService(ConnectivityManager::class.java)

    companion object {
        /** Set on the intent that opens the app to ask for the microphone permission. */
        const val EXTRA_REQUEST_MICROPHONE = "dev.yuyudhan.simplevoice.mobile.REQUEST_MICROPHONE"

        private const val DEFAULT_MAX_SECONDS = 300
        private const val DOUBLE_TAP_MS = 300L
        private const val NOTICE_MS = 2500L
        private const val METER_MS = 80L
        private const val WORD_LOOKBACK = 48
        private const val SUGGESTIONS = 3
        private val CLOSING_PUNCTUATION = setOf(".", ",", "?", "!", ":", ";")
    }
}
