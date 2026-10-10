// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/KeyFeedback.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.content.Context
import android.media.AudioManager
import android.view.HapticFeedbackConstants
import android.view.View

/**
 * The tick and click a key gives the moment a finger lands on it, not when it lifts, so typing
 * feels connected to the glass. Both follow the user's system settings: the haptic constant obeys
 * "Touch feedback", and the one-argument [AudioManager.playSoundEffect] plays only while "Touch
 * sounds" is on (and, being a system sound, stays quiet in silent and vibrate modes).
 */
internal class KeyFeedback(context: Context) {
    private val audio = context.getSystemService(AudioManager::class.java)

    fun press(view: View, key: Key?) {
        view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
        val sound = when (key) {
            Key.Space -> AudioManager.FX_KEYPRESS_SPACEBAR
            Key.Backspace -> AudioManager.FX_KEYPRESS_DELETE
            Key.Return -> AudioManager.FX_KEYPRESS_RETURN
            else -> AudioManager.FX_KEYPRESS_STANDARD
        }
        audio?.playSoundEffect(sound)
    }

    /** A held key switching to its second meaning: firmer than a tap, and silent. */
    fun hold(view: View) {
        view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
    }

    /** One step of the space-bar cursor drag. */
    fun tick(view: View) {
        view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
    }
}
