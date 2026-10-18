// FilePath: mobile/plugins/sv-native/android/src/main/java/dev/yuyudhan/simplevoice/svnative/KeyboardRequest.kt
// How the Simple Voice keyboard asks the app for a setup step it cannot do itself: a keyboard
// shows no permission prompt and has no room for the Groq key, so a tap on its status line opens
// MainActivity with EXTRA set, and the app's Settings tab takes it (`takeKeyboardRequest`).

package dev.yuyudhan.simplevoice.svnative

object KeyboardRequest {
    const val EXTRA = "dev.yuyudhan.simplevoice.mobile.KEYBOARD_REQUEST"

    /** Ask for the microphone, then go back to the field the keyboard was typing in. */
    const val MICROPHONE = "microphone"

    /** Show the Groq key field. */
    const val GROQ_KEY = "groqKey"
}
