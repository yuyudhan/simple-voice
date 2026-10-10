// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/KeyView.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.util.TypedValue
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.accessibility.AccessibilityEvent
import android.view.animation.DecelerateInterpolator
import androidx.core.graphics.ColorUtils
import kotlin.math.abs

/** How a key's cap is filled: a letter key, a quiet function key, or the field's action. */
internal enum class KeyStyle { LETTER, FUNCTION, ACTION }

/** What a key shows: a label or an icon, and the long-press character in its corner. */
internal data class KeyFace(
    val style: KeyStyle,
    val label: String = "",
    val labelSp: Float = 0f,
    val labelColour: Int = 0,
    val medium: Boolean = false,
    val icon: Drawable? = null,
    val iconColour: Int = 0,
    val hint: String? = null,
)

/** What a key needs from the keyboard around it. */
internal interface KeyHost {
    val palette: Palette
    val feedback: KeyFeedback
    fun onKey(key: Key)
    fun onCapsLock()
    fun onCursor(offset: Int)
    fun showPreview(anchor: KeyView, text: String, alternate: Boolean)
    fun hidePreview(anchor: KeyView)
}

/**
 * One key, drawn rather than built from stock views so its cap, edge, label and touch area are
 * exact. The cap sits inside the view with half the gap on each side; the gap still belongs to
 * the view, so a tap between two keys goes to the nearer one instead of being lost. [leading]
 * and [trailing] are empty widths (in key units) that widen the touch area past the cap, as on
 * the short middle letter row.
 */
internal class KeyView(
    context: Context,
    private val host: KeyHost,
    val key: Key,
    private val leading: Float = 0f,
    private val trailing: Float = 0f,
) : View(context) {
    /** The cap, in this view's coordinates. */
    val cap = RectF()

    var face = KeyFace(KeyStyle.LETTER)
        set(value) {
            if (field == value) return
            field = value
            invalidate()
        }

    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val edge = RectF()
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply { textAlign = Paint.Align.CENTER }
    private val hintText = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        textAlign = Paint.Align.RIGHT
        textSize = sp(HINT_SP)
    }
    private val glyph = Rect()
    private val radius = dp(RADIUS_DP)
    private val edgeHeight = dp(1f)

    /** 1 while a finger is on the key, fading to 0 after it lifts. */
    private var pressLevel = 0f
    private var fade: ValueAnimator? = null
    private var held = false
    private var downX = 0f
    private var moved = 0
    private var dragging = false
    private val holdable = (key as? Key.Text)?.alt != null || key == Key.Shift
    private val hold = Runnable { onHold() }
    private val repeat = object : Runnable {
        override fun run() {
            host.onKey(Key.Backspace)
            postDelayed(this, REPEAT_INTERVAL_MS)
        }
    }

    init {
        isClickable = true
        isLongClickable = holdable
        // A keyboard never takes focus from the field it types into.
        isFocusable = false
        hintText.color = host.palette.hint
    }

    override fun onSizeChanged(width: Int, height: Int, oldWidth: Int, oldHeight: Int) {
        val unit = width / (1f + leading + trailing)
        val gapX = dp(GAP_X_DP) / 2
        val gapY = dp(GAP_Y_DP) / 2
        cap.set(unit * leading + gapX, gapY, width - unit * trailing - gapX, height - gapY - edgeHeight)
    }

    override fun onDraw(canvas: Canvas) {
        val palette = host.palette
        val (idle, pressed) = when (face.style) {
            KeyStyle.LETTER -> palette.key to palette.keyPressed
            KeyStyle.FUNCTION -> palette.functionKey to palette.functionPressed
            KeyStyle.ACTION -> palette.accent to palette.accentPressed
        }
        // The darker 1 dp edge under the cap is the key's only depth: no outline, no shadow.
        edge.set(cap)
        edge.offset(0f, edgeHeight)
        fill.color = palette.keyEdge
        canvas.drawRoundRect(edge, radius, radius, fill)
        fill.color = ColorUtils.blendARGB(idle, pressed, pressLevel)
        canvas.drawRoundRect(cap, radius, radius, fill)

        val icon = face.icon
        if (icon != null) {
            val half = dp(ICON_DP) / 2
            icon.setBounds(
                (cap.centerX() - half).toInt(),
                (cap.centerY() - half).toInt(),
                (cap.centerX() + half).toInt(),
                (cap.centerY() + half).toInt(),
            )
            icon.setTint(face.iconColour)
            icon.draw(canvas)
        } else if (face.label.isNotEmpty()) {
            text.textSize = sp(face.labelSp)
            text.color = face.labelColour
            text.typeface = if (face.medium) MEDIUM else REGULAR
            // Centre on the x-height for lower-case letters and on the cap height otherwise, so
            // ascenders and descenders do not pull a letter off the middle of its key.
            val reference = if (face.label.length == 1 && face.label[0].isLowerCase()) "x" else "H"
            text.getTextBounds(reference, 0, 1, glyph)
            canvas.drawText(face.label, cap.centerX(), cap.centerY() + glyph.height() / 2f, text)
        }

        val hint = face.hint ?: return
        hintText.getTextBounds("H", 0, 1, glyph)
        canvas.drawText(hint, cap.right - dp(HINT_INSET_DP), cap.top + dp(HINT_INSET_DP) + glyph.height(), hintText)
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> down(event)
            MotionEvent.ACTION_MOVE -> if (key == Key.Space) drag(event)
            MotionEvent.ACTION_UP -> up()
            MotionEvent.ACTION_CANCEL -> stop()
        }
        return true
    }

    /** A tap, from the touch path or an accessibility service. */
    override fun performClick(): Boolean {
        super.performClick()
        host.onKey(key)
        return true
    }

    /** The long-press meaning, for accessibility services; touches use [onHold]. */
    override fun performLongClick(): Boolean {
        val alt = (key as? Key.Text)?.alt
        when {
            alt != null -> host.onKey(Key.Text(alt))
            key == Key.Shift -> host.onCapsLock()
            else -> return false
        }
        sendAccessibilityEvent(AccessibilityEvent.TYPE_VIEW_LONG_CLICKED)
        return true
    }

    override fun onDetachedFromWindow() {
        stop()
        super.onDetachedFromWindow()
    }

    private fun down(event: MotionEvent) {
        fade?.cancel()
        pressLevel = 1f
        invalidate()
        host.feedback.press(this, key)
        held = false
        when (key) {
            // Delete acts on touch so a held key starts repeating from the first character.
            Key.Backspace -> {
                host.onKey(key)
                postDelayed(repeat, REPEAT_DELAY_MS)
            }
            Key.Space -> {
                downX = event.x
                moved = 0
                dragging = false
            }
            is Key.Text -> host.showPreview(this, face.label, alternate = false)
            else -> Unit
        }
        if (holdable) postDelayed(hold, ViewConfiguration.getLongPressTimeout().toLong())
    }

    /** A drag along the space bar moves the cursor one character per [STEP_DP]. */
    private fun drag(event: MotionEvent) {
        val distance = event.x - downX
        if (!dragging && abs(distance) > dp(DRAG_START_DP)) dragging = true
        if (!dragging) return
        val steps = (distance / dp(STEP_DP)).toInt()
        if (steps == moved) return
        host.feedback.tick(this)
        host.onCursor(steps - moved)
        moved = steps
    }

    private fun up() {
        val alt = (key as? Key.Text)?.alt
        val wasHeld = held
        val wasDragging = dragging
        stop()
        when {
            key == Key.Backspace -> Unit
            key == Key.Space && wasDragging -> Unit
            wasHeld && alt != null -> host.onKey(Key.Text(alt))
            // Shift's hold has already locked the capitals.
            wasHeld -> Unit
            else -> performClick()
        }
    }

    /** Ends a touch: the key fades back over Signal's 150 ms and any popup goes. */
    private fun stop() {
        removeCallbacks(hold)
        removeCallbacks(repeat)
        held = false
        dragging = false
        host.hidePreview(this)
        if (pressLevel == 0f) return
        fade?.cancel()
        fade = ValueAnimator.ofFloat(pressLevel, 0f).apply {
            duration = RELEASE_MS
            interpolator = DecelerateInterpolator()
            addUpdateListener {
                pressLevel = it.animatedValue as Float
                invalidate()
            }
            start()
        }
    }

    private fun onHold() {
        held = true
        host.feedback.hold(this)
        val alt = (key as? Key.Text)?.alt
        if (alt != null) host.showPreview(this, alt, alternate = true) else host.onCapsLock()
    }

    private fun dp(value: Float): Float =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value, resources.displayMetrics)

    private fun dp(value: Int): Float = dp(value.toFloat())

    private fun sp(value: Float): Float =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, value, resources.displayMetrics)

    companion object {
        /** Gaps between caps; with the row height they set the keys' proportions. */
        const val GAP_X_DP = 6
        const val GAP_Y_DP = 10
        const val RADIUS_DP = 6f
        const val ICON_DP = 22
        const val HINT_SP = 10f
        const val HINT_INSET_DP = 4
        const val RELEASE_MS = 150L
        const val REPEAT_DELAY_MS = 400L
        const val REPEAT_INTERVAL_MS = 60L
        const val STEP_DP = 12
        const val DRAG_START_DP = 10
        private val REGULAR: Typeface = Typeface.create("sans-serif", Typeface.NORMAL)
        private val MEDIUM: Typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
    }
}
