// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/Palette.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.content.res.Configuration

/**
 * Signal colours from src/styles/tokens.css; keep both in step. The keyboard is a surface with
 * keys lifted one step above it, so each role names the token it uses in each theme.
 */
internal data class Palette(
    /** Keyboard background: light `--surface`, dark `--bg`. */
    val background: Int,
    /** Letter keys: light `--panel`, dark `--surface-hover`. */
    val key: Int,
    /** Shift, backspace, layer and return keys: light `--surface-hover`, dark `--border-strong`. */
    val functionKey: Int,
    val border: Int,
    val text: Int,
    val muted: Int,
    val accent: Int,
    val onAccent: Int,
    val success: Int,
    val warning: Int,
    val danger: Int,
) {
    companion object {
        fun of(configuration: Configuration): Palette {
            val night = configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
            return if (night == Configuration.UI_MODE_NIGHT_YES) DARK else LIGHT
        }

        private val LIGHT = Palette(
            background = 0xFFF4F4F5.toInt(),
            key = 0xFFFFFFFF.toInt(),
            functionKey = 0xFFEBEBED.toInt(),
            border = 0xFFE6E6E9.toInt(),
            text = 0xFF151518.toInt(),
            muted = 0xFF62626B.toInt(),
            accent = 0xFFF0521A.toInt(),
            onAccent = 0xFFFFFFFF.toInt(),
            success = 0xFF1C7F47.toInt(),
            warning = 0xFF8F6200.toInt(),
            danger = 0xFFC42639.toInt(),
        )

        private val DARK = Palette(
            background = 0xFF111113.toInt(),
            key = 0xFF222226.toInt(),
            functionKey = 0xFF3A3A41.toInt(),
            border = 0xFF29292E.toInt(),
            text = 0xFFEDEDF0.toInt(),
            muted = 0xFFA2A2AB.toInt(),
            accent = 0xFFFF6A2E.toInt(),
            onAccent = 0xFF1A0B04.toInt(),
            success = 0xFF5FD08F.toInt(),
            warning = 0xFFF0C05A.toInt(),
            danger = 0xFFFF6B7A.toInt(),
        )
    }
}
