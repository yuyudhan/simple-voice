// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/KeyboardView.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.content.Context
import android.content.res.Configuration
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.InsetDrawable
import android.graphics.drawable.StateListDrawable
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import androidx.core.view.WindowInsetsCompat
import dev.yuyudhan.simplevoice.mobile.R

/** What the top bar's dot and text say. */
internal enum class Tone { READY, BLOCKED, RECORDING, WORKING, RETRY }

/** What the return key does in this field, and the icon that says so. */
internal enum class ReturnAction(val icon: Int, val description: String) {
    NEWLINE(R.drawable.sv_return, "Return"),
    SEND(R.drawable.sv_send, "Send"),
    SEARCH(R.drawable.sv_search, "Search"),
    GO(R.drawable.sv_arrow_right, "Go"),
    NEXT(R.drawable.sv_arrow_right, "Next"),
    DONE(R.drawable.sv_check, "Done"),
}

internal interface KeyboardListener {
    fun onKey(key: Key)
    fun onMic()
    fun onStatusTap()
    fun onSuggestion(word: String)

    /** Moves the cursor [offset] characters: a drag along the space bar. */
    fun onCursor(offset: Int)

    /** Turns caps lock on: a long press on shift, besides the double tap. */
    fun onCapsLock()
}

/**
 * The Simple Voice keyboard's views: a top bar (status or suggestions, and the mic), the key
 * rows, and the key preview floating over both. It draws whatever state [KeyboardService] gives
 * it and reports taps back.
 */
internal class KeyboardView(
    context: Context,
    override val palette: Palette,
    private val listener: KeyboardListener,
) : FrameLayout(context), KeyHost {
    override val feedback = KeyFeedback(context)
    private val preview = KeyPreview(context, palette)
    private val statusDot = View(context)
    private val levels = LevelBars(context, palette.accent)
    private val statusText = TextView(context)
    private val mic = MicButton(context, palette, feedback)
    private val status = LinearLayout(context)
    private val suggestions = LinearLayout(context)
    private val rows = LinearLayout(context)
    private val keys = mutableListOf<KeyView>()
    private val icons = mutableMapOf<Int, Drawable?>()
    private val landscape = resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    private var layer = Layer.LETTERS
    private var shift = ShiftState.OFF
    private var returnAction = ReturnAction.NEWLINE

    /** Recording, sending and Retry keep the bar; suggestions wait until they are done. */
    private var statusPinned = false

    init {
        setBackgroundColor(palette.background)
        val content = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(SIDE_DP), 0, dp(SIDE_DP), dp(BOTTOM_DP))
            addView(topBar(), LinearLayout.LayoutParams(MATCH, dp(BAR_DP)))
            addView(rows, LinearLayout.LayoutParams(MATCH, WRAP))
        }
        rows.orientation = LinearLayout.VERTICAL
        addView(content, LayoutParams(MATCH, WRAP))
        addView(preview, LayoutParams(MATCH, MATCH))
        // Android draws the keyboard behind the navigation bar; keep the keys above it.
        setOnApplyWindowInsetsListener { target, insets ->
            val bottom = WindowInsetsCompat.toWindowInsetsCompat(insets, target)
                .getInsets(WindowInsetsCompat.Type.navigationBars())
                .bottom
            target.setPadding(0, 0, 0, bottom)
            insets
        }
        render()
    }

    fun setLayer(layer: Layer, shift: ShiftState) {
        if (this.layer == layer && this.shift == shift) return
        val rebuild = this.layer != layer
        this.layer = layer
        this.shift = shift
        // Shift only changes faces; rebuilding would cut short a key's press fade.
        if (rebuild) render() else refresh()
    }

    fun setReturnAction(action: ReturnAction) {
        if (returnAction == action) return
        returnAction = action
        refresh()
    }

    fun setStatus(line: String, tone: Tone) {
        statusText.text = line
        val colour = when (tone) {
            Tone.READY -> palette.success
            Tone.BLOCKED -> palette.danger
            Tone.RECORDING -> palette.accent
            Tone.WORKING -> palette.muted
            Tone.RETRY -> palette.warning
        }
        (statusDot.background as? GradientDrawable)?.setColor(colour)
        // A live recording and a waiting Retry are read at a glance; the rest is background.
        statusText.setTextColor(if (tone == Tone.RECORDING || tone == Tone.RETRY) palette.text else palette.muted)
        // While recording, the meter replaces the dot and shows the voice being heard.
        val recording = tone == Tone.RECORDING
        statusDot.visibility = if (recording) GONE else VISIBLE
        if (recording != (levels.visibility == VISIBLE)) {
            levels.clear()
            levels.visibility = if (recording) VISIBLE else GONE
        }
        statusPinned = tone == Tone.RECORDING || tone == Tone.WORKING || tone == Tone.RETRY
        showBar()
    }

    /** The microphone's latest level, from 0 to 1. */
    fun setLevel(level: Float) {
        levels.push(level)
        mic.setLevel(level)
    }

    fun setMic(state: MicState) {
        mic.state = state
    }

    fun setSuggestions(words: List<String>) {
        suggestions.removeAllViews()
        val shown = words.take(SUGGESTIONS)
        // The best guess sits in the middle, where the eye already is, as on Gboard and iOS.
        val order = if (shown.size == SUGGESTIONS) listOf(shown[1], shown[0], shown[2]) else shown
        val best = if (shown.size == SUGGESTIONS) 1 else 0
        order.forEachIndexed { index, word ->
            if (index > 0) suggestions.addView(divider(), LinearLayout.LayoutParams(dp(1), dp(DIVIDER_DP)))
            suggestions.addView(suggestion(word, index == best), LinearLayout.LayoutParams(0, MATCH, 1f))
        }
        showBar()
    }

    override fun onKey(key: Key) = listener.onKey(key)

    override fun onCapsLock() = listener.onCapsLock()

    override fun onCursor(offset: Int) = listener.onCursor(offset)

    override fun showPreview(anchor: KeyView, text: String, alternate: Boolean) =
        preview.show(anchor, text, alternate)

    override fun hidePreview(anchor: KeyView) = preview.hide(anchor)

    /** One bar beside the mic: suggestions while a word is being typed, the status otherwise. */
    private fun showBar() {
        val words = suggestions.childCount > 0 && !statusPinned
        suggestions.visibility = if (words) VISIBLE else GONE
        status.visibility = if (words) GONE else VISIBLE
    }

    private fun suggestion(word: String, best: Boolean): View = TextView(context).apply {
        text = word
        gravity = Gravity.CENTER
        maxLines = 1
        ellipsize = TextUtils.TruncateAt.END
        setPadding(dp(6), 0, dp(6), 0)
        setTextColor(if (best) palette.text else palette.muted)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, SUGGESTION_SP)
        typeface = Typeface.create(if (best) "sans-serif-medium" else "sans-serif", Typeface.NORMAL)
        background = pressedPill()
        isFocusable = false
        isSoundEffectsEnabled = false
        setOnTouchListener { view, event ->
            if (event.actionMasked == MotionEvent.ACTION_DOWN) feedback.press(view, null)
            false
        }
        setOnClickListener { listener.onSuggestion(word) }
    }

    /** Clear at rest, a rounded function-key fill under the finger. */
    private fun pressedPill(): StateListDrawable {
        val pressed = GradientDrawable().apply {
            cornerRadius = dp(KeyView.RADIUS_DP.toInt()).toFloat()
            setColor(palette.functionKey)
        }
        return StateListDrawable().apply {
            addState(intArrayOf(android.R.attr.state_pressed), InsetDrawable(pressed, 0, dp(6), 0, dp(6)))
        }
    }

    private fun divider(): View = View(context).apply { setBackgroundColor(palette.divider) }

    private fun topBar(): View {
        statusDot.background = GradientDrawable().apply { shape = GradientDrawable.OVAL }
        levels.visibility = GONE
        statusText.apply {
            setTextColor(palette.muted)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, STATUS_SP)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
            // Centres the line on the dot; font padding would sit it a pixel or two low.
            includeFontPadding = false
            // The recording timer ticks every second; tabular figures keep it from jittering.
            fontFeatureSettings = "tnum"
        }
        status.apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(8), 0, dp(8), 0)
            addView(statusDot, LinearLayout.LayoutParams(dp(8), dp(8)).apply { marginEnd = dp(10) })
            addView(levels, LinearLayout.LayoutParams(dp(20), dp(16)).apply { marginEnd = dp(10) })
            addView(statusText, LinearLayout.LayoutParams(0, WRAP, 1f))
            isFocusable = false
            setOnClickListener { listener.onStatusTap() }
        }
        suggestions.apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            visibility = GONE
        }
        mic.setOnClickListener { listener.onMic() }
        return LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(status, LinearLayout.LayoutParams(0, MATCH, 1f))
            addView(suggestions, LinearLayout.LayoutParams(0, MATCH, 1f).apply { marginEnd = dp(4) })
            addView(mic, LinearLayout.LayoutParams(dp(BAR_DP), dp(BAR_DP)))
        }
    }

    private fun render() {
        rows.removeAllViews()
        keys.clear()
        val height = dp(if (landscape) ROW_LANDSCAPE_DP else ROW_DP)
        for (row in KeyLayout.rows(layer)) {
            val line = LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
            // The middle letter row is half a key in from each side, as on every phone keyboard;
            // its end keys take that space as touch area so a tap there still types.
            val short = layer == Layer.LETTERS && row.size == 9
            row.forEachIndexed { index, key ->
                val leading = if (short && index == 0) 0.5f else 0f
                val trailing = if (short && index == row.lastIndex) 0.5f else 0f
                val view = KeyView(context, this, key, leading, trailing)
                keys.add(view)
                line.addView(view, LinearLayout.LayoutParams(0, height, weight(key) + leading + trailing))
            }
            rows.addView(line, LinearLayout.LayoutParams(MATCH, WRAP))
        }
        refresh()
    }

    /** Updates every key's face for the current shift and return action. */
    private fun refresh() {
        for (view in keys) {
            view.face = face(view.key)
            view.contentDescription = description(view.key)
        }
    }

    /** The bottom row adds up to the same width in every layer, so its keys do not jump. */
    private fun weight(key: Key): Float = when (key) {
        is Key.Text -> 1f
        Key.Space -> if (layer == Layer.LETTERS) 4f else 6f
        Key.Return -> 1.8f
        Key.Shift, Key.Backspace -> 1.5f
        Key.NextKeyboard -> 1.1f
        is Key.SwitchLayer -> 1.4f
    }

    /**
     * Letter keys are bright and labelled large; function keys sit a step darker with quiet
     * icons. Shift lifts to a letter key while on, with an accent arrow when locked (Signal's
     * "selected" marker); return takes the accent only when it sends, searches or goes.
     */
    private fun face(key: Key): KeyFace = when (key) {
        is Key.Text -> KeyFace(KeyStyle.LETTER, label(key), LETTER_SP, palette.text, hint = key.alt)
        Key.Space -> KeyFace(KeyStyle.LETTER, "space", SPACE_SP, palette.muted)
        is Key.SwitchLayer -> KeyFace(KeyStyle.FUNCTION, key.label, FUNCTION_SP, palette.text, medium = true)
        Key.Shift -> when (shift) {
            ShiftState.OFF -> iconFace(KeyStyle.FUNCTION, R.drawable.sv_shift, palette.text)
            ShiftState.ONCE -> iconFace(KeyStyle.LETTER, R.drawable.sv_shift_once, palette.text)
            ShiftState.LOCKED -> iconFace(KeyStyle.LETTER, R.drawable.sv_caps_lock, palette.accent)
        }
        Key.Backspace -> iconFace(KeyStyle.FUNCTION, R.drawable.sv_backspace, palette.text)
        Key.NextKeyboard -> iconFace(KeyStyle.FUNCTION, R.drawable.sv_globe, palette.text)
        Key.Return -> if (returnAction == ReturnAction.NEWLINE) {
            iconFace(KeyStyle.FUNCTION, returnAction.icon, palette.text)
        } else {
            iconFace(KeyStyle.ACTION, returnAction.icon, palette.onAccent)
        }
    }

    private fun iconFace(style: KeyStyle, icon: Int, colour: Int) =
        KeyFace(style, icon = icons.getOrPut(icon) { ContextCompat.getDrawable(context, icon)?.mutate() }, iconColour = colour)

    private fun label(key: Key.Text): String = if (shift != ShiftState.OFF) key.text.uppercase() else key.text

    private fun description(key: Key): String = when (key) {
        is Key.Text -> key.text
        Key.Shift -> when (shift) {
            ShiftState.OFF -> "Shift"
            ShiftState.ONCE -> "Shift on"
            ShiftState.LOCKED -> "Caps lock"
        }
        Key.Backspace -> "Delete"
        Key.Space -> "Space"
        Key.Return -> returnAction.description
        Key.NextKeyboard -> "Next keyboard"
        is Key.SwitchLayer -> key.label
    }

    private fun dp(value: Int): Int = TypedValue.applyDimension(
        TypedValue.COMPLEX_UNIT_DIP,
        value.toFloat(),
        resources.displayMetrics,
    ).toInt()

    private companion object {
        const val MATCH = LayoutParams.MATCH_PARENT
        const val WRAP = LayoutParams.WRAP_CONTENT
        const val SIDE_DP = 3
        const val BOTTOM_DP = 6
        const val BAR_DP = 48
        const val ROW_DP = 54
        const val ROW_LANDSCAPE_DP = 44
        const val DIVIDER_DP = 20
        const val SUGGESTIONS = 3
        const val LETTER_SP = 22f
        const val SPACE_SP = 14f
        const val FUNCTION_SP = 15f
        const val STATUS_SP = 13f
        const val SUGGESTION_SP = 16f
    }
}
