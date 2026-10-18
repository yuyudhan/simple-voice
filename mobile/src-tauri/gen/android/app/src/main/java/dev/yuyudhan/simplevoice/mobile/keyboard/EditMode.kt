// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/EditMode.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import java.io.File

/**
 * One edit (MB-6): the selected text, the recording of how to change it, and where it was made.
 * [historyId] is the failed attempt's history row once there is one, 0 before.
 */
internal data class EditRequest(
    val selection: String,
    val file: File,
    val durationMs: Long = 0,
    val appName: String? = null,
    val appId: String? = null,
    val historyId: Long = 0,
)

/** What a tap on "Not sent · tap to retry" runs again. */
internal sealed interface Retry {
    data class Dictation(val id: Long) : Retry

    /** An edit keeps its audio and selection here, not in history, which cannot retry it. */
    data class Edit(val request: EditRequest) : Retry
}

/**
 * Edit mode: with text selected the mic shows a pen, the recording is the instruction and the
 * core's answer replaces the selection in place. The selection comes from
 * `InputConnection.getSelectedText`, which some apps answer with nothing or a shortened copy;
 * such a selection is refused rather than half replaced.
 */
internal object EditMode {
    const val UNREADABLE = "This app doesn't share the selected text"
    const val NOTHING_SELECTED = "Select the text to edit first"
    const val EDITED = "Edited"
    const val SELECTION_CHANGED = "Copied · the selection changed"
    const val SELECTION_CHANGED_REASON = "The selection changed first; the edit was copied"

    /** The core's own wording, which it uses when it refuses a selection. */
    fun tooLong(maxChars: Int) = "Select at most $maxChars characters to edit"

    /**
     * Why [selected] cannot be edited, or null when it can. [selectedLength] is the selection's
     * length as the field last reported it, in UTF-16 units like [String.length]; a read shorter
     * than that is a copy the app cut short. The limit counts characters, as the core does.
     */
    fun problem(selected: String?, selectedLength: Int, maxChars: Int): String? = when {
        selected == null || selected.length < selectedLength ->
            if (selectedLength > maxChars) tooLong(maxChars) else UNREADABLE
        selected.codePointCount(0, selected.length) > maxChars -> tooLong(maxChars)
        selected.isBlank() -> NOTHING_SELECTED
        else -> null
    }

    /** A new edit recording. The prefix marks audio that only a running keyboard can use. */
    fun file(filesDir: File) = File(filesDir, "audio/$PREFIX${System.currentTimeMillis()}.wav")

    /** Deletes edit audio an earlier keyboard process left: its selections went with it. */
    fun sweep(filesDir: File) {
        File(filesDir, "audio").listFiles { file -> file.name.startsWith(PREFIX) }?.forEach { it.delete() }
    }

    /** Sends [request] to the core; blocks on Groq, so call it off the main thread. */
    fun send(request: EditRequest): String = SvCore.edit(
        request.selection,
        request.file.absolutePath,
        request.durationMs,
        request.appName,
        request.appId,
        request.historyId,
    )

    private const val PREFIX = "edit-"
}
