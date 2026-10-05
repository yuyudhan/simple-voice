// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/KeyboardView.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.StateListDrawable
import android.os.Handler
import android.os.Looper
import android.util.TypedValue
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import dev.yuyudhan.simplevoice.mobile.R
import kotlin.math.abs

/** What the top bar's dot and text say. */
internal enum class Tone { READY, BLOCKED, RECORDING, WORKING, RETRY }

/** The mic button: tap to start, tap to stop (MB-3), or unavailable. */
internal enum class MicState { IDLE, DISABLED, RECORDING, BUSY }

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
 * The Simple Voice keyboard's views: a top bar (status and mic), a suggestion strip and the key
 * rows. It draws whatever state [KeyboardService] gives it and reports taps back.
 */
internal class KeyboardView(
    context: Context,
    private val palette: Palette,
    private val listener: KeyboardListener,
) : LinearLayout(context) {
    private val main = Handler(Looper.getMainLooper())
    private val statusDot = View(context)
    private val levels = LevelBars(context, palette.accent)
    private val statusText = TextView(context)
    private val mic = ImageView(context)
    private val status = LinearLayout(context)
    private val suggestions = LinearLayout(context)
    private val rows = LinearLayout(context)
    private var layer = Layer.LETTERS
    private var shift = ShiftState.OFF
    private var returnAction = ReturnAction.NEWLINE

    /** Recording, sending and Retry keep the bar; suggestions wait until they are done. */
    private var statusPinned = false

    init {
        orientation = VERTICAL
        setBackgroundColor(palette.background)
        setPadding(dp(4), dp(6), dp(4), dp(6))
        addView(topBar(), LayoutParams(LayoutParams.MATCH_PARENT, dp(44)))
        rows.orientation = VERTICAL
        addView(rows, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
        render()
    }

    fun setLayer(layer: Layer, shift: ShiftState) {
        if (this.layer == layer && this.shift == shift) return
        this.layer = layer
        this.shift = shift
        render()
    }

    fun setReturnAction(action: ReturnAction) {
        if (returnAction == action) return
        returnAction = action
        render()
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
    fun setLevel(level: Float) = levels.push(level)

    fun setMic(state: MicState) {
        mic.setImageResource(if (state == MicState.RECORDING) R.drawable.sv_stop else R.drawable.sv_mic)
        val background = mic.background as? GradientDrawable
        background?.setColor(if (state == MicState.DISABLED) palette.functionKey else palette.accent)
        mic.imageTintList = ColorStateList.valueOf(
            if (state == MicState.DISABLED) palette.muted else palette.onAccent,
        )
        mic.alpha = if (state == MicState.BUSY) 0.5f else 1f
        mic.contentDescription = if (state == MicState.RECORDING) "Stop dictating" else "Dictate"
    }

    fun setSuggestions(words: List<String>) {
        suggestions.removeAllViews()
        words.take(3).forEachIndexed { index, word ->
            if (index > 0) suggestions.addView(divider(), LayoutParams(dp(1), dp(18)))
            val cell = TextView(context).apply {
                text = word
                gravity = Gravity.CENTER
                maxLines = 1
                setTextColor(palette.text)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
                if (index == 0) setTypeface(typeface, Typeface.BOLD)
                background = pressable(palette.background, palette.functionKey, stroke = false)
                setOnClickListener {
                    it.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                    listener.onSuggestion(word)
                }
            }
            suggestions.addView(cell, LayoutParams(0, LayoutParams.MATCH_PARENT, 1f))
        }
        showBar()
    }

    /** One bar beside the mic: suggestions while a word is being typed, the status otherwise. */
    private fun showBar() {
        val words = suggestions.childCount > 0 && !statusPinned
        suggestions.visibility = if (words) VISIBLE else GONE
        status.visibility = if (words) GONE else VISIBLE
    }

    private fun divider(): View = View(context).apply { setBackgroundColor(palette.border) }

    private fun topBar(): View {
        statusDot.background = GradientDrawable().apply { shape = GradientDrawable.OVAL }
        levels.visibility = GONE
        statusText.apply {
            setTextColor(palette.muted)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            maxLines = 1
        }
        mic.apply {
            background = GradientDrawable().apply { shape = GradientDrawable.OVAL }
            scaleType = ImageView.ScaleType.CENTER
            setOnClickListener {
                it.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                listener.onMic()
            }
        }
        setMic(MicState.DISABLED)
        status.apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(statusDot, LayoutParams(dp(7), dp(7)).apply { marginEnd = dp(8) })
            addView(levels, LayoutParams(dp(22), dp(16)).apply { marginEnd = dp(8) })
            addView(statusText, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
            setOnClickListener { listener.onStatusTap() }
        }
        suggestions.apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            visibility = GONE
        }
        return LinearLayout(context).apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(8), 0, dp(4), 0)
            addView(status, LayoutParams(0, LayoutParams.MATCH_PARENT, 1f))
            addView(suggestions, LayoutParams(0, LayoutParams.MATCH_PARENT, 1f).apply { marginEnd = dp(8) })
            addView(mic, LayoutParams(dp(38), dp(38)))
        }
    }

    private fun render() {
        rows.removeAllViews()
        for (row in KeyLayout.rows(layer)) {
            val line = LinearLayout(context).apply {
                orientation = HORIZONTAL
                gravity = Gravity.CENTER_HORIZONTAL
            }
            // The middle letter row is narrower, as on every phone keyboard.
            val inset = if (layer == Layer.LETTERS && row.size == 9) 0.5f else 0f
            if (inset > 0f) line.addView(View(context), LayoutParams(0, 1, inset))
            for (key in row) line.addView(keyView(key), LayoutParams(0, dp(48), weight(key)))
            if (inset > 0f) line.addView(View(context), LayoutParams(0, 1, inset))
            rows.addView(line, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
        }
    }

    private fun weight(key: Key): Float = when (key) {
        is Key.Text -> 1f
        Key.Space -> 4f
        Key.Return -> 1.8f
        Key.Shift, Key.Backspace -> 1.5f
        Key.NextKeyboard -> 1.1f
        is Key.SwitchLayer -> 1.4f
    }

    private fun keyView(key: Key): View {
        val function = key !is Key.Text && key != Key.Space
        // Only caps lock fills the key with the accent; one-letter shift just fills the arrow.
        val shiftOn = key == Key.Shift && shift == ShiftState.LOCKED
        val idle = when {
            shiftOn -> palette.accent
            function -> palette.functionKey
            else -> palette.key
        }
        val icon = icon(key)
        val cap: View = if (icon != null) {
            // Every symbol key is a drawn icon in one stroke style (docs/internal/design.md); font
            // arrows render small and the emoji globe in colour.
            ImageView(context).apply {
                setImageResource(icon)
                scaleType = ImageView.ScaleType.CENTER
                imageTintList = ColorStateList.valueOf(if (shiftOn) palette.onAccent else palette.text)
            }
        } else {
            TextView(context).apply {
                gravity = Gravity.CENTER
                text = label(key)
                setTextColor(palette.text)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, if (key is Key.Text) 21f else 14f)
            }
        }
        cap.background = pressable(idle, if (function) palette.key else palette.functionKey)
        cap.contentDescription = description(key)
        val holder = FrameLayout(context).apply {
            setPadding(dp(2), dp(3), dp(2), dp(3))
            addView(cap, FrameLayout.LayoutParams(MATCH, MATCH))
        }
        val alt = (key as? Key.Text)?.alt
        if (alt != null) holder.addView(hint(alt), FrameLayout.LayoutParams(WRAP, WRAP, Gravity.TOP or Gravity.END))
        when {
            key == Key.Backspace -> cap.setOnTouchListener { view, event -> repeatBackspace(view, event) }
            key == Key.Space -> cap.setOnTouchListener(SpaceDrag())
            else -> cap.setOnClickListener {
                it.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                listener.onKey(key)
            }
        }
        if (alt != null) {
            cap.setOnLongClickListener {
                it.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                listener.onKey(Key.Text(alt))
                true
            }
        }
        if (key == Key.Shift) {
            cap.setOnLongClickListener {
                it.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                listener.onCapsLock()
                true
            }
        }
        return holder
    }

    /** The long-press character, small in the key's top-right corner. */
    private fun hint(alt: String): View = TextView(context).apply {
        text = alt
        setTextColor(palette.muted)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 10f)
        setPadding(0, dp(4), dp(6), 0)
        // Taps go to the key underneath.
        isClickable = false
        isFocusable = false
    }

    /** A key face that darkens (or lightens, on function keys) while pressed. */
    private fun pressable(idle: Int, pressed: Int, stroke: Boolean = true): StateListDrawable {
        fun face(colour: Int) = GradientDrawable().apply {
            cornerRadius = dp(6).toFloat()
            setColor(colour)
            if (stroke) setStroke(dp(1), palette.border)
        }
        return StateListDrawable().apply {
            addState(intArrayOf(android.R.attr.state_pressed), face(pressed))
            addState(intArrayOf(), face(idle))
        }
    }

    private val repeat = object : Runnable {
        override fun run() {
            listener.onKey(Key.Backspace)
            main.postDelayed(this, REPEAT_INTERVAL_MS)
        }
    }

    /** One delete on touch, then a steady repeat while the key is held. */
    private fun repeatBackspace(view: View, event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                view.isPressed = true
                view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                listener.onKey(Key.Backspace)
                main.postDelayed(repeat, REPEAT_DELAY_MS)
            }
            MotionEvent.ACTION_UP -> {
                view.isPressed = false
                main.removeCallbacks(repeat)
                // Lets accessibility services see the press as a click.
                view.performClick()
            }
            MotionEvent.ACTION_CANCEL -> {
                view.isPressed = false
                main.removeCallbacks(repeat)
            }
        }
        return true
    }

    /** A tap types a space; a drag left or right moves the cursor, one step per [STEP_DP]. */
    private inner class SpaceDrag : OnTouchListener {
        private var startX = 0f
        private var moved = 0
        private var dragging = false

        override fun onTouch(view: View, event: MotionEvent): Boolean {
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    view.isPressed = true
                    startX = event.x
                    moved = 0
                    dragging = false
                }
                MotionEvent.ACTION_MOVE -> {
                    val steps = ((event.x - startX) / dp(STEP_DP)).toInt()
                    if (!dragging && abs(event.x - startX) > dp(DRAG_START_DP)) dragging = true
                    if (dragging && steps != moved) {
                        view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                        listener.onCursor(steps - moved)
                        moved = steps
                    }
                }
                MotionEvent.ACTION_UP -> {
                    view.isPressed = false
                    if (!dragging) {
                        view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                        listener.onKey(Key.Space)
                    }
                    view.performClick()
                }
                MotionEvent.ACTION_CANCEL -> view.isPressed = false
            }
            return true
        }
    }

    /** Shift: outline when off, filled for one letter, filled with a bar under it when locked. */
    private fun icon(key: Key): Int? = when (key) {
        Key.Shift -> when (shift) {
            ShiftState.OFF -> R.drawable.sv_shift
            ShiftState.ONCE -> R.drawable.sv_shift_once
            ShiftState.LOCKED -> R.drawable.sv_caps_lock
        }
        Key.NextKeyboard -> R.drawable.sv_globe
        Key.Backspace -> R.drawable.sv_backspace
        Key.Return -> returnAction.icon
        else -> null
    }

    private fun label(key: Key): String = when (key) {
        is Key.Text -> if (shift != ShiftState.OFF) key.text.uppercase() else key.text
        Key.Space -> "space"
        is Key.SwitchLayer -> key.label
        Key.Shift, Key.NextKeyboard, Key.Backspace, Key.Return -> ""
    }

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
        const val MATCH = FrameLayout.LayoutParams.MATCH_PARENT
        const val WRAP = FrameLayout.LayoutParams.WRAP_CONTENT
        const val REPEAT_DELAY_MS = 400L
        const val REPEAT_INTERVAL_MS = 60L
        const val STEP_DP = 12
        const val DRAG_START_DP = 10
    }
}
