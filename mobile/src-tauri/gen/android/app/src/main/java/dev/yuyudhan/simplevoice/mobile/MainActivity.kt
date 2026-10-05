package dev.yuyudhan.simplevoice.mobile

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import dev.yuyudhan.simplevoice.mobile.keyboard.KeyboardService

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    requestMicrophoneFor(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    requestMicrophoneFor(intent)
  }

  // A keyboard cannot show permission prompts, so the Simple Voice keyboard opens the app to ask.
  private fun requestMicrophoneFor(intent: Intent?) {
    if (intent?.getBooleanExtra(KeyboardService.EXTRA_REQUEST_MICROPHONE, false) != true) return
    if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) return
    requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), MICROPHONE_REQUEST)
  }

  private companion object {
    const val MICROPHONE_REQUEST = 1
  }
}
