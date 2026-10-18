// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/KeyboardStatus.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import dev.yuyudhan.simplevoice.mobile.MainActivity
import dev.yuyudhan.simplevoice.svnative.KeyboardRequest
import org.json.JSONObject

/** The setup step a tap on the status line opens in the app. */
internal enum class Fix(val request: String) {
    GROQ_KEY(KeyboardRequest.GROQ_KEY),
    MICROPHONE(KeyboardRequest.MICROPHONE),
}

/** What the top bar says while nothing is recording or sending, and the core's limits. */
internal data class Status(
    val line: String,
    val ready: Boolean,
    val fix: Fix?,
    val maxSeconds: Int,
    val editMaxChars: Int,
)

/**
 * The top bar's state: the Rust core's answer (Groq key, connection) plus the microphone
 * permission, which Android can take back at any time: "Only this time" lapses about a minute
 * after leaving the app, with the keyboard open or not. A blocked mic never opens the app; only a
 * tap on the status line does ([openApp]), since a keyboard cannot show a permission prompt.
 */
internal class KeyboardStatus(private val context: Context, onNetworkChange: () -> Unit) {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)

    private val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) =
            onNetworkChange()

        override fun onLost(network: Network) = onNetworkChange()
    }

    fun start() = connectivity.registerDefaultNetworkCallback(networkCallback)

    fun stop() = connectivity.unregisterNetworkCallback(networkCallback)

    fun microphoneAllowed(): Boolean =
        context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    /** Blocks on the Rust core (it reads the database): call it off the main thread. */
    fun read(): Status {
        val core = JSONObject(SvCore.status(isOnline()))
        return decide(
            line = core.optString("line"),
            ready = core.optBoolean("ready"),
            needsKey = core.optBoolean("needsKey"),
            microphone = microphoneAllowed(),
            maxSeconds = core.optInt("maxSeconds", DEFAULT_MAX_SECONDS),
            editMaxChars = core.optInt("editMaxChars"),
        )
    }

    /** Opens the app on the setup step behind [fix]: the Groq key field or the microphone. */
    fun openApp(fix: Fix) {
        val intent = Intent(context, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            .putExtra(KeyboardRequest.EXTRA, fix.request)
        context.startActivity(intent)
    }

    /** Online means a validated connection, not just Wi-Fi joined behind a login page. */
    private fun isOnline(): Boolean {
        val capabilities = connectivity.getNetworkCapabilities(connectivity.activeNetwork) ?: return false
        return capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
    }

    companion object {
        const val DEFAULT_MAX_SECONDS = 300
        const val GROQ_KEY_MISSING = "No Groq key · tap here to add it"
        const val MICROPHONE_OFF = "Microphone off · tap here to allow"

        /**
         * Setup gaps come before the connection: the key first, then the microphone, then the
         * core's [line] (online, offline). A setup gap says where to tap, since the mic will not.
         */
        fun decide(
            line: String,
            ready: Boolean,
            needsKey: Boolean,
            microphone: Boolean,
            maxSeconds: Int,
            editMaxChars: Int,
        ): Status {
            val seconds = maxSeconds.takeIf { it > 0 } ?: DEFAULT_MAX_SECONDS
            return when {
                needsKey -> Status(GROQ_KEY_MISSING, false, Fix.GROQ_KEY, seconds, editMaxChars)
                !microphone -> Status(MICROPHONE_OFF, false, Fix.MICROPHONE, seconds, editMaxChars)
                else -> Status(line, ready, null, seconds, editMaxChars)
            }
        }
    }
}
