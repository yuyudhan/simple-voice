// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/LevelBars.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.view.View

/**
 * Meter bars, Signal's unit for sound (docs/internal/design.md): the last few microphone levels
 * as rounded vertical bars, newest on the right, so the speaker sees they are being heard.
 */
internal class LevelBars(context: Context, colour: Int) : View(context) {
    private val levels = FloatArray(BARS)
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = colour }
    private val bar = RectF()

    /** Adds a level from 0 (silence) to 1 (full scale). */
    fun push(level: Float) {
        levels.copyInto(levels, 0, 1, BARS)
        levels[BARS - 1] = level.coerceIn(0f, 1f)
        invalidate()
    }

    fun clear() {
        levels.fill(0f)
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val gap = width / (BARS * 3f)
        val barWidth = (width - gap * (BARS - 1)) / BARS
        val radius = barWidth / 2
        for (index in 0 until BARS) {
            // Speech sits far below full scale; the square root lifts it into view.
            val visible = kotlin.math.sqrt(levels[index])
            val barHeight = (MIN_HEIGHT + (1 - MIN_HEIGHT) * visible) * height
            val left = index * (barWidth + gap)
            val top = (height - barHeight) / 2
            bar.set(left, top, left + barWidth, top + barHeight)
            canvas.drawRoundRect(bar, radius, radius, paint)
        }
    }

    private companion object {
        const val BARS = 5
        const val MIN_HEIGHT = 0.2f
    }
}
