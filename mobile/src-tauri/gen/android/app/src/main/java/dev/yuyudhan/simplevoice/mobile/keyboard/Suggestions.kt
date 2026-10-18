// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/Suggestions.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.view.textservice.SentenceSuggestionsInfo
import android.view.textservice.SpellCheckerSession
import android.view.textservice.SuggestionsInfo
import android.view.textservice.TextInfo
import android.view.textservice.TextServicesManager

/**
 * Spelling suggestions for the word being typed, from the system spell checker. [show] gets
 * each answer on the main thread, best guess first.
 */
internal class Suggestions(context: Context, private val show: (List<String>) -> Unit) :
    SpellCheckerSession.SpellCheckerSessionListener {
    private val main = Handler(Looper.getMainLooper())
    private val session = context.getSystemService(TextServicesManager::class.java)
        ?.newSpellCheckerSession(null, null, this, true)

    /** The word being typed, which a chosen suggestion replaces. */
    var word = ""
        private set

    /** Asks about the word that ends [before], the text before the cursor. */
    fun update(before: String) {
        word = before.takeLastWhile { it.isLetter() || it == '\'' }
        val checker = session
        if (word.length < 2 || checker == null) {
            show(emptyList())
            return
        }
        checker.getSentenceSuggestions(arrayOf(TextInfo(word)), COUNT)
    }

    fun close() {
        session?.close()
    }

    override fun onGetSuggestions(results: Array<out SuggestionsInfo>?) = Unit

    override fun onGetSentenceSuggestions(results: Array<out SentenceSuggestionsInfo>?) {
        val words = mutableListOf<String>()
        val sentence = results?.firstOrNull()
        if (sentence != null && sentence.suggestionsCount > 0) {
            val info = sentence.getSuggestionsInfoAt(0)
            for (index in 0 until info.suggestionsCount) words.add(info.getSuggestionAt(index))
        }
        main.post { show(words) }
    }

    companion object {
        /** How much text before the cursor [update] needs to find the word. */
        const val LOOKBACK = 48
        private const val COUNT = 3
    }
}
