// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/SvCore.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

/**
 * The Rust core as seen from the keyboard (crates/sv-ffi over crates/sv-mobile). The keyboard
 * runs while the app's screen is closed, so Tauri is not running; these functions reach the same
 * library directly. Answers are JSON. [dictate], [retry] and [edit] call Groq and block: call
 * them off the main thread.
 */
object SvCore {
    init {
        System.loadLibrary("simple_voice_mobile_lib")
    }

    /** Opens the shared database in [dataDir]; returns an empty string or the error message. */
    external fun init(dataDir: String): String

    /** `{line, ready, needsKey, maxSeconds, editMaxChars}`: the top bar before speaking. */
    external fun status(online: Boolean): String

    /**
     * Transcribes and formats the WAV at [wavPath] and saves it to history. Returns
     * `{kind: "text", id, text, unformatted}`, `{kind: "noSpeech"}` or `{kind: "failed", id?, message}`.
     */
    external fun dictate(wavPath: String, durationMs: Long, appName: String?, appId: String?): String

    /** Retries failed dictation [id] from its kept audio; the same answers as [dictate]. */
    external fun retry(id: Long): String

    /**
     * Rewrites [selection] by the instruction recorded at [wavPath] and saves it to history;
     * [retryOf] is the failed attempt's history row, or 0. Returns `{kind: "text", id, text}`
     * (the replacement), `{kind: "noSpeech"}` or `{kind: "failed", id?, message}`; with an `id`
     * the audio is kept for a retry, otherwise it is deleted.
     */
    external fun edit(
        selection: String,
        wavPath: String,
        durationMs: Long,
        appName: String?,
        appId: String?,
        retryOf: Long,
    ): String

    /** Records that dictation [id]'s text never reached its field. */
    external fun markNotInserted(id: Long, reason: String)
}
