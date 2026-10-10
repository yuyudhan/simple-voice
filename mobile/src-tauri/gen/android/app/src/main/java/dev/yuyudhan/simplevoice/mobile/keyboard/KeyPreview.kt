// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/KeyPreview.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Typeface
import android.util.TypedValue
import android.view.View

/**
 * The bubble that floats above a key while it is held: the letter under the finger, large, so
 * the typist sees what they hit; on a long press it shows the key's second character in the
 * accent instead. It is the keyboard's one floating thing, so it is the one place with a shadow
 * (docs/internal/design.md). The view covers the whole keyboard and takes no touches.
 */
internal class KeyPreview(context: Context, private val palette: Palette) : View(context) {
    private val bubble = RectF()
    private val glyph = Rect()
    private val here = IntArray(2)
    private val there = IntArray(2)
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = palette.popup
        setShadowLayer(dp(SHADOW_DP), 0f, dp(SHADOW_Y_DP), palette.popupShadow)
    }
    private val hairline = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = dp(1f)
        color = palette.popupEdge
    }
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        textAlign = Paint.Align.CENTER
        textSize = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, TEXT_SP, resources.displayMetrics)
        typeface = Typeface.create("sans-serif", Typeface.NORMAL)
    }
    private var anchor: KeyView? = null
    private var label = ""
    private val hide = Runnable {
        anchor = null
        invalidate()
    }

    init {
        isClickable = false
        isFocusable = false
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
    }

    fun show(key: KeyView, value: String, alternate: Boolean) {
        removeCallbacks(hide)
        anchor = key
        label = value
        text.color = if (alternate) palette.accentText else palette.text
        // Place the bubble over the key's cap, in this view's coordinates.
        getLocationInWindow(here)
        key.getLocationInWindow(there)
        val left = there[0] - here[0] + key.cap.left
        val top = there[1] - here[1] + key.cap.top
        val width = maxOf(key.cap.width() + dp(WIDEN_DP), dp(MIN_WIDTH_DP))
        val height = dp(HEIGHT_DP)
        val centre = left + key.cap.width() / 2
        val x = (centre - width / 2).coerceIn(dp(EDGE_DP), this.width - dp(EDGE_DP) - width)
        val y = (top - dp(LIFT_DP) - height).coerceAtLeast(dp(EDGE_DP))
        bubble.set(x, y, x + width, y + height)
        invalidate()
    }

    /** Lets the bubble linger a moment after the finger lifts, so a quick tap still shows it. */
    fun hide(key: KeyView) {
        if (anchor !== key) return
        removeCallbacks(hide)
        postDelayed(hide, LINGER_MS)
    }

    /**
     * Takes the keyboard's size without setting it: asked to wrap, it wants nothing, and the
     * frame then measures it again at exactly its own size.
     */
    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        setMeasuredDimension(exact(widthMeasureSpec), exact(heightMeasureSpec))
    }

    private fun exact(spec: Int): Int =
        if (MeasureSpec.getMode(spec) == MeasureSpec.EXACTLY) MeasureSpec.getSize(spec) else 0

    override fun onDraw(canvas: Canvas) {
        if (anchor == null) return
        val radius = dp(RADIUS_DP)
        canvas.drawRoundRect(bubble, radius, radius, fill)
        canvas.drawRoundRect(bubble, radius, radius, hairline)
        val reference = if (label.length == 1 && label[0].isLowerCase()) "x" else "H"
        text.getTextBounds(reference, 0, 1, glyph)
        canvas.drawText(label, bubble.centerX(), bubble.centerY() + glyph.height() / 2f, text)
    }

    override fun onDetachedFromWindow() {
        removeCallbacks(hide)
        super.onDetachedFromWindow()
    }

    private fun dp(value: Float): Float =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value, resources.displayMetrics)

    private companion object {
        const val TEXT_SP = 30f
        const val HEIGHT_DP = 54f
        const val WIDEN_DP = 14f
        const val MIN_WIDTH_DP = 48f
        const val LIFT_DP = 4f
        const val EDGE_DP = 2f
        const val RADIUS_DP = 10f
        const val SHADOW_DP = 8f
        const val SHADOW_Y_DP = 2f
        const val LINGER_MS = 60L
    }
}
