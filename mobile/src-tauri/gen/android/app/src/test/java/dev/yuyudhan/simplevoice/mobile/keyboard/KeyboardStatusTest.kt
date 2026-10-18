// FilePath: mobile/src-tauri/gen/android/app/src/test/java/dev/yuyudhan/simplevoice/mobile/keyboard/KeyboardStatusTest.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import org.junit.Assert.assertEquals
import org.junit.Test

class KeyboardStatusTest {
    private fun decide(ready: Boolean, needsKey: Boolean, microphone: Boolean) = KeyboardStatus.decide(
        line = "Groq Whisper · online",
        ready = ready,
        needsKey = needsKey,
        microphone = microphone,
        maxSeconds = 120,
    )

    @Test
    fun aRevokedMicrophoneBlocksDictationEvenWhenTheCoreIsReady() {
        assertEquals(
            Status(KeyboardStatus.MICROPHONE_OFF, false, Fix.MICROPHONE, 120),
            decide(ready = true, needsKey = false, microphone = false),
        )
    }

    @Test
    fun aMissingKeyIsReportedBeforeTheMicrophone() {
        assertEquals(
            Status(KeyboardStatus.GROQ_KEY_MISSING, false, Fix.GROQ_KEY, 120),
            decide(ready = false, needsKey = true, microphone = false),
        )
    }

    @Test
    fun withKeyAndMicrophoneTheCoreLineStandsAndNothingOpensTheApp() {
        assertEquals(
            Status("Groq Whisper · online", true, null, 120),
            decide(ready = true, needsKey = false, microphone = true),
        )
    }
}
