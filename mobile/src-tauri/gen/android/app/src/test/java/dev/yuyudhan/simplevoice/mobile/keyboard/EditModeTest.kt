// FilePath: mobile/src-tauri/gen/android/app/src/test/java/dev/yuyudhan/simplevoice/mobile/keyboard/EditModeTest.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class EditModeTest {
    @Test
    fun aSelectionAtTheLimitIsEditedAndOneCharacterMoreIsRefused() {
        val atLimit = "a".repeat(MAX)
        assertNull(EditMode.problem(atLimit, atLimit.length, MAX))
        val over = atLimit + "b"
        assertEquals(EditMode.tooLong(MAX), EditMode.problem(over, over.length, MAX))
    }

    @Test
    fun theLimitCountsCharactersNotUtf16Units() {
        // Each emoji is two UTF-16 units but one character, as the core counts them.
        val emoji = "\uD83D\uDE00".repeat(MAX)
        assertNull(EditMode.problem(emoji, emoji.length, MAX))
    }

    @Test
    fun aMissingOrShortenedReadIsNeverHalfReplaced() {
        assertEquals(EditMode.UNREADABLE, EditMode.problem(null, 12, MAX))
        assertEquals(EditMode.UNREADABLE, EditMode.problem("only part", 40, MAX))
        // A cut-short copy of an over-long selection still says what to fix.
        assertEquals(EditMode.tooLong(MAX), EditMode.problem("a".repeat(100), MAX + 1, MAX))
        assertEquals(EditMode.tooLong(MAX), EditMode.problem(null, MAX + 1, MAX))
    }

    @Test
    fun aBlankSelectionHasNothingToEdit() {
        assertEquals(EditMode.NOTHING_SELECTED, EditMode.problem(" \n", 2, MAX))
    }

    private companion object {
        const val MAX = 4000
    }
}
