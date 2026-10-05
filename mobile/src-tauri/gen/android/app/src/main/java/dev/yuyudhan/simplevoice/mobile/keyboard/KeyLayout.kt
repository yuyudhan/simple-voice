// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/KeyLayout.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

/** One key on the keyboard. [Text] types its text; the others act. */
internal sealed class Key {
    /** Types [text]; a long press types [alt] instead, shown small in the key's corner. */
    data class Text(val text: String, val alt: String? = null) : Key()
    data object Shift : Key()
    data object Backspace : Key()
    data object Space : Key()
    data object Return : Key()
    data object NextKeyboard : Key()
    data class SwitchLayer(val layer: Layer, val label: String) : Key()
}

internal enum class Layer { LETTERS, NUMBERS, SYMBOLS }

/** Off, the next letter only, or every letter until tapped again (a double tap). */
internal enum class ShiftState { OFF, ONCE, LOCKED }

/**
 * The rows of each layer, top to bottom: a phone QWERTY with `123` and `#+=` layers. The top
 * letter row holds the digits on a long press, and comma and full stop sit beside the space bar
 * so a sentence never needs a layer switch.
 */
internal object KeyLayout {
    fun rows(layer: Layer): List<List<Key>> = when (layer) {
        Layer.LETTERS -> listOf(
            "qwertyuiop".mapIndexed { index, letter ->
                Key.Text(letter.toString(), ((index + 1) % 10).toString())
            },
            text("asdfghjkl"),
            listOf(Key.Shift) + text("zxcvbnm") + Key.Backspace,
            listOf(
                Key.SwitchLayer(Layer.NUMBERS, "123"),
                Key.NextKeyboard,
                Key.Text(",", "!"),
                Key.Space,
                Key.Text(".", "?"),
                Key.Return,
            ),
        )
        Layer.NUMBERS -> listOf(
            text("1234567890"),
            texts("-", "/", ":", ";", "(", ")", "$", "&", "@", "\""),
            listOf(Key.SwitchLayer(Layer.SYMBOLS, "#+=")) +
                texts(".", ",", "?", "!", "'") + Key.Backspace,
            bottom(Key.SwitchLayer(Layer.LETTERS, "ABC")),
        )
        Layer.SYMBOLS -> listOf(
            texts("[", "]", "{", "}", "#", "%", "^", "*", "+", "="),
            texts("_", "\\", "|", "~", "<", ">", "€", "£", "¥", "•"),
            listOf(Key.SwitchLayer(Layer.NUMBERS, "123")) +
                texts(".", ",", "?", "!", "'") + Key.Backspace,
            bottom(Key.SwitchLayer(Layer.LETTERS, "ABC")),
        )
    }

    private fun bottom(layerKey: Key): List<Key> =
        listOf(layerKey, Key.NextKeyboard, Key.Space, Key.Return)

    private fun text(letters: String): List<Key> = letters.map { Key.Text(it.toString()) }

    private fun texts(vararg values: String): List<Key> = values.map { Key.Text(it) }
}
