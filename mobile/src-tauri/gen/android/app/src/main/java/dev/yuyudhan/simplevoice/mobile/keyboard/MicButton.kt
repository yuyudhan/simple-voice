// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/MicButton.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.util.TypedValue
import android.view.MotionEvent
import android.view.View
import androidx.core.content.ContextCompat
import dev.yuyudhan.simplevoice.mobile.R
import kotlin.math.sqrt

/** The mic button: tap to start, tap to stop (MB-3), or unavailable. */
internal enum class MicState { IDLE, DISABLED, RECORDING, BUSY }

/**
 * The top bar's mic: a signal-orange disc, grey while dictation cannot start. While it records
 * it shows stop and wears a halo that swells with the voice, so "live" is obvious at a glance
 * even with the status text out of the eye line. With text selected it shows a pen: the next
 * recording edits the selection (MB-6). Disc and halo fit the 48 dp bar.
 */
internal class MicButton(
    context: Context,
    private val palette: Palette,
    private val feedback: KeyFeedback,
) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val mic = ContextCompat.getDrawable(context, R.drawable.sv_mic)?.mutate()
    private val stop = ContextCompat.getDrawable(context, R.drawable.sv_stop)?.mutate()
    private val pen = ContextCompat.getDrawable(context, R.drawable.sv_pen)?.mutate()

    /** The voice level, smoothed so the halo breathes instead of flickering. */
    private var level = 0f

    var state = MicState.DISABLED
        set(value) {
            field = value
            if (value != MicState.RECORDING) level = 0f
            alpha = if (value == MicState.BUSY) BUSY_ALPHA else 1f
            describe()
            invalidate()
        }

    /** Text is selected, so the mic edits it instead of dictating. */
    var editing = false
        set(value) {
            if (field == value) return
            field = value
            describe()
            invalidate()
        }

    init {
        isFocusable = false
        // The key-press click comes from [feedback]; the stock click sound would double it.
        isSoundEffectsEnabled = false
        state = MicState.DISABLED
    }

    /** The microphone's latest level, from 0 to 1. */
    fun setLevel(value: Float) {
        // Speech sits far below full scale; the square root lifts it into view.
        level += (sqrt(value.coerceIn(0f, 1f)) - level) * SMOOTHING
        invalidate()
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (event.actionMasked == MotionEvent.ACTION_DOWN && isEnabled) feedback.press(this, null)
        return super.onTouchEvent(event)
    }

    override fun drawableStateChanged() {
        super.drawableStateChanged()
        invalidate()
    }

    private fun describe() {
        contentDescription = when {
            state == MicState.RECORDING -> if (editing) "Stop and edit" else "Stop dictating"
            editing -> "Edit the selection by voice"
            else -> "Dictate"
        }
    }

    override fun onDraw(canvas: Canvas) {
        val cx = width / 2f
        val cy = height / 2f
        val radius = dp(RADIUS_DP)
        if (state == MicState.RECORDING) {
            paint.color = palette.halo
            canvas.drawCircle(cx, cy, radius + dp(HALO_DP) + level * dp(HALO_SWELL_DP), paint)
        }
        paint.color = when {
            state == MicState.DISABLED -> if (isPressed) palette.functionPressed else palette.functionKey
            isPressed -> palette.accentPressed
            else -> palette.accent
        }
        canvas.drawCircle(cx, cy, radius, paint)
        val icon = when {
            state == MicState.RECORDING -> stop
            editing -> pen
            else -> mic
        } ?: return
        val half = dp(ICON_DP) / 2
        icon.setBounds((cx - half).toInt(), (cy - half).toInt(), (cx + half).toInt(), (cy + half).toInt())
        icon.setTint(if (state == MicState.DISABLED) palette.muted else palette.onAccent)
        icon.draw(canvas)
    }

    private fun dp(value: Float): Float =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value, resources.displayMetrics)

    private companion object {
        const val RADIUS_DP = 18f
        const val ICON_DP = 22f
        const val HALO_DP = 3f
        const val HALO_SWELL_DP = 3f
        const val SMOOTHING = 0.5f
        const val BUSY_ALPHA = 0.5f
    }
}
