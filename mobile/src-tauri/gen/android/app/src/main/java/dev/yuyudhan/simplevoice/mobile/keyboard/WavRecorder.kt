// FilePath: mobile/src-tauri/gen/android/app/src/main/java/dev/yuyudhan/simplevoice/mobile/keyboard/WavRecorder.kt
package dev.yuyudhan.simplevoice.mobile.keyboard

import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import java.io.File
import java.io.RandomAccessFile

/**
 * Records the microphone into [file] as a 16 kHz mono PCM16 WAV, the format Groq Whisper and
 * the desktop app use. The caller checks the RECORD_AUDIO permission before [start].
 */
internal class WavRecorder(private val file: File) {
    private var thread: Thread? = null

    @Volatile
    private var running = false

    @Volatile
    private var samples = 0L

    /** The loudest sample of the last buffer, from 0 to 1, for the keyboard's meter. */
    @Volatile
    var level = 0f
        private set

    /** Starts capturing; false when the microphone cannot be opened. */
    fun start(): Boolean {
        val minBuffer = AudioRecord.getMinBufferSize(SAMPLE_RATE, CHANNEL, ENCODING)
        if (minBuffer <= 0) return false
        val recorder = try {
            AudioRecord(
                MediaRecorder.AudioSource.VOICE_RECOGNITION,
                SAMPLE_RATE,
                CHANNEL,
                ENCODING,
                minBuffer * 4,
            )
        } catch (error: SecurityException) {
            return false
        } catch (error: IllegalArgumentException) {
            return false
        }
        if (recorder.state != AudioRecord.STATE_INITIALIZED) {
            recorder.release()
            return false
        }
        file.parentFile?.mkdirs()
        val out = RandomAccessFile(file, "rw")
        out.setLength(0)
        // Room for the header, written once the length is known.
        out.write(ByteArray(HEADER_BYTES))
        samples = 0
        running = true
        recorder.startRecording()
        thread = Thread {
            val buffer = ShortArray(minBuffer)
            val bytes = ByteArray(minBuffer * 2)
            while (running) {
                val read = recorder.read(buffer, 0, buffer.size)
                if (read <= 0) continue
                var peak = 0
                for (index in 0 until read) {
                    val sample = buffer[index].toInt()
                    bytes[index * 2] = (sample and 0xFF).toByte()
                    bytes[index * 2 + 1] = ((sample shr 8) and 0xFF).toByte()
                    peak = maxOf(peak, kotlin.math.abs(sample))
                }
                level = peak / Short.MAX_VALUE.toFloat()
                out.write(bytes, 0, read * 2)
                samples += read
            }
            recorder.stop()
            recorder.release()
            out.seek(0)
            out.write(header(samples))
            out.close()
        }.apply { start() }
        return true
    }

    /** Stops, finishes the file and returns the recording's length in milliseconds. */
    fun stop(): Long {
        running = false
        thread?.join()
        thread = null
        return samples * 1000 / SAMPLE_RATE
    }

    /** Stops and deletes the file: the recording is abandoned. */
    fun cancel() {
        stop()
        file.delete()
    }

    private fun header(sampleCount: Long): ByteArray {
        val dataBytes = sampleCount * 2
        val header = ByteArray(HEADER_BYTES)
        fun text(offset: Int, value: String) = value.toByteArray().copyInto(header, offset)
        fun int(offset: Int, value: Long) {
            for (index in 0 until 4) header[offset + index] = ((value shr (8 * index)) and 0xFF).toByte()
        }
        fun short(offset: Int, value: Int) {
            header[offset] = (value and 0xFF).toByte()
            header[offset + 1] = ((value shr 8) and 0xFF).toByte()
        }
        text(0, "RIFF")
        int(4, 36 + dataBytes)
        text(8, "WAVE")
        text(12, "fmt ")
        int(16, 16)
        short(20, 1) // PCM
        short(22, 1) // mono
        int(24, SAMPLE_RATE.toLong())
        int(28, SAMPLE_RATE.toLong() * 2) // byte rate
        short(32, 2) // block align
        short(34, 16) // bits per sample
        text(36, "data")
        int(40, dataBytes)
        return header
    }

    private companion object {
        const val SAMPLE_RATE = 16_000
        const val CHANNEL = AudioFormat.CHANNEL_IN_MONO
        const val ENCODING = AudioFormat.ENCODING_PCM_16BIT
        const val HEADER_BYTES = 44
    }
}
