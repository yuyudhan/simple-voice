// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/Palette.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.content.res.Configuration

/**
 * Signal colours from src/styles/tokens.css; keep both in step. The keyboard is a well with keys
 * lifted one step above it, so each role names the token it uses in each theme. Letter keys are
 * the brightest step (as on a phone keyboard, so the eye finds them first); function keys sit
 * between them and the well.
 */
internal data class Palette(
    /** Keyboard well: light `--surface-hover`, dark `--surface`. */
    val background: Int,
    /** Letter and space keys: light `--panel`, dark `--border-strong`. */
    val key: Int,
    /** A letter key held down: light `--border-strong`, dark `--faint`. */
    val keyPressed: Int,
    /** Shift, delete, layer, globe and plain return: light `--border-strong`, dark `--border`. */
    val functionKey: Int,
    /** A function key held down: light `--panel`, dark `--border-strong`. */
    val functionPressed: Int,
    /** The 1 dp edge under every key that lifts it off the well: `--backdrop`. */
    val keyEdge: Int,
    /** Hairlines between suggestions: `--border-strong`. */
    val divider: Int,
    val text: Int,
    val muted: Int,
    /** The long-press character in a key's corner: light `--faint`, dark `--muted`. */
    val hint: Int,
    val accent: Int,
    /** The accent held down: `--accent-hover`. */
    val accentPressed: Int,
    /** Accent-coloured text on the keys and popups: `--accent-text`. */
    val accentText: Int,
    /** The mic's recording halo, a step down the signal ramp: `--accent-3`. */
    val halo: Int,
    val onAccent: Int,
    /** The key preview floating above a key: the letter key's colour, as on the key itself. */
    val popup: Int,
    /** The popup's hairline and shadow: the two parts of `--shadow-overlay`. */
    val popupEdge: Int,
    val popupShadow: Int,
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
            background = 0xFFEBEBED.toInt(),
            key = 0xFFFFFFFF.toInt(),
            keyPressed = 0xFFD2D2D7.toInt(),
            functionKey = 0xFFD2D2D7.toInt(),
            functionPressed = 0xFFFFFFFF.toInt(),
            keyEdge = 0x47151518,
            divider = 0xFFD2D2D7.toInt(),
            text = 0xFF151518.toInt(),
            muted = 0xFF62626B.toInt(),
            hint = 0xFF9D9DA6.toInt(),
            accent = 0xFFF0521A.toInt(),
            accentPressed = 0xFFD9460F.toInt(),
            accentText = 0xFFB93A0B.toInt(),
            halo = 0xFFFAB393.toInt(),
            onAccent = 0xFFFFFFFF.toInt(),
            popup = 0xFFFFFFFF.toInt(),
            popupEdge = 0x0F151518,
            popupShadow = 0x29151518,
            success = 0xFF1C7F47.toInt(),
            warning = 0xFF8F6200.toInt(),
            danger = 0xFFC42639.toInt(),
        )

        private val DARK = Palette(
            background = 0xFF18181B.toInt(),
            key = 0xFF3A3A41.toInt(),
            keyPressed = 0xFF66666F.toInt(),
            functionKey = 0xFF29292E.toInt(),
            functionPressed = 0xFF3A3A41.toInt(),
            keyEdge = 0x8C000000.toInt(),
            divider = 0xFF3A3A41.toInt(),
            text = 0xFFEDEDF0.toInt(),
            muted = 0xFFA2A2AB.toInt(),
            hint = 0xFFA2A2AB.toInt(),
            accent = 0xFFFF6A2E.toInt(),
            accentPressed = 0xFFFF8352.toInt(),
            accentText = 0xFFFF8A5C.toInt(),
            halo = 0xFF9C3A14.toInt(),
            onAccent = 0xFF1A0B04.toInt(),
            popup = 0xFF3A3A41.toInt(),
            popupEdge = 0x0FFFFFFF,
            popupShadow = 0x8C000000.toInt(),
            success = 0xFF5FD08F.toInt(),
            warning = 0xFFF0C05A.toInt(),
            danger = 0xFFFF6B7A.toInt(),
        )
    }
}
