package dev.yuyudhan.simplevoice.mobile

import android.content.Intent
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  // A tap on the keyboard's status line reopens this activity with the setup step it needs; the
  // Settings tab reads it from the current intent (sv-native `takeKeyboardRequest`) and asks for
  // the microphone through the plugin, the one place that handles a permanent denial.
  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
  }
}
