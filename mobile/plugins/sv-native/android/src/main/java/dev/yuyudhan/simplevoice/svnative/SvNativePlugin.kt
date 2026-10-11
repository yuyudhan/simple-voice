// FilePath: mobile/plugins/sv-native/android/src/main/java/dev/yuyudhan/simplevoice/svnative/SvNativePlugin.kt
// Keyboard setup from inside the app: reports how far setup has got and opens the system pages
// that finish it. The Rust side (src/setup.rs) calls these commands with an empty payload.

package dev.yuyudhan.simplevoice.svnative

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject

private const val KEYBOARD_ID = "dev.yuyudhan.simplevoice.mobile/.keyboard.KeyboardService"
private const val MICROPHONE = "microphone"
private const val PREFS = "sv_native_setup"
private const val ASKED_MICROPHONE = "asked_microphone"
private const val PROMPTABLE = "microphone_promptable"

@TauriPlugin(
    permissions = [Permission(strings = [Manifest.permission.RECORD_AUDIO], alias = MICROPHONE)]
)
class SvNativePlugin(private val activity: Activity) : Plugin(activity) {
    private val prefs = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    @Command
    fun setupStatus(invoke: Invoke) {
        val result = JSObject()
        result.put("enabled", keyboardEnabled())
        // Full Access is an iPhone concept; Rust decodes null as "not applicable".
        result.put("fullAccess", JSONObject.NULL)
        result.put("selected", keyboardSelected())
        result.put("microphone", microphoneState())
        invoke.resolve(result)
    }

    @Command
    fun openKeyboardSettings(invoke: Invoke) {
        activity.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
        invoke.resolve(JSObject())
    }

    @Command
    fun chooseKeyboard(invoke: Invoke) {
        // The picker only shows while our window has focus, so post it to the UI thread.
        activity.runOnUiThread {
            inputMethodManager().showInputMethodPicker()
            invoke.resolve(JSObject())
        }
    }

    @Command
    fun requestMicrophone(invoke: Invoke) {
        if (microphoneGranted()) {
            resolveMicrophone(invoke, "granted")
            return
        }
        // Android answers "denied" instantly, without a dialog, once the user said no for good.
        // The rationale flag before asking is how we tell that apart in the callback.
        val askedBefore = prefs.getBoolean(ASKED_MICROPHONE, false)
        val canPrompt = !askedBefore || shouldShowRationale()
        prefs.edit()
            .putBoolean(ASKED_MICROPHONE, true)
            .putBoolean(PROMPTABLE, canPrompt)
            .apply()
        requestPermissionForAlias(MICROPHONE, invoke, "microphoneAnswered")
    }

    @PermissionCallback
    private fun microphoneAnswered(invoke: Invoke) {
        if (microphoneGranted()) {
            resolveMicrophone(invoke, "granted")
            return
        }
        // No dialog could have been shown (and none will next time): only the app's own
        // settings page can turn the microphone back on.
        val wasPromptable = prefs.getBoolean(PROMPTABLE, true)
        if (!wasPromptable && !shouldShowRationale()) {
            val intent = Intent(
                Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                Uri.fromParts("package", activity.packageName, null)
            )
            activity.startActivity(intent)
        }
        resolveMicrophone(invoke, "denied")
    }

    private fun resolveMicrophone(invoke: Invoke, state: String) {
        val result = JSObject()
        result.put("microphone", state)
        invoke.resolve(result)
    }

    private fun microphoneState(): String = when {
        microphoneGranted() -> "granted"
        // `pm revoke` or a never-asked install both land here; the prompt can still show.
        !prefs.getBoolean(ASKED_MICROPHONE, false) -> "undetermined"
        else -> "denied"
    }

    private fun microphoneGranted(): Boolean =
        ContextCompat.checkSelfPermission(activity, Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED

    private fun shouldShowRationale(): Boolean =
        ActivityCompat.shouldShowRequestPermissionRationale(activity, Manifest.permission.RECORD_AUDIO)

    private fun keyboardEnabled(): Boolean =
        inputMethodManager().enabledInputMethodList.any { it.id == KEYBOARD_ID }

    private fun keyboardSelected(): Boolean =
        Settings.Secure.getString(activity.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD) ==
            KEYBOARD_ID

    private fun inputMethodManager(): InputMethodManager =
        activity.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
}
