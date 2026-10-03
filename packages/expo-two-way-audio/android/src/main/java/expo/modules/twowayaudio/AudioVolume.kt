package expo.modules.twowayaudio

import kotlin.math.log10
import kotlin.math.max
import kotlin.math.pow
import kotlin.math.sqrt

/** Convert little-endian signed PCM16 samples to the volume meter's normalized level. */
internal fun pcm16VolumeLevel(buffer: ByteArray): Float {
    val samples = buffer.size / 2
    if (samples == 0) return 0.0f
    var squares = 0.0
    for (index in 0 until samples) {
        // The low byte must not sign-extend into the high byte.
        val sample = ((buffer[index * 2].toInt() and 0xff) or
            (buffer[index * 2 + 1].toInt() shl 8)).toShort()
        val normalized = sample / 32768.0
        squares += normalized * normalized
    }
    val rms = sqrt(squares / samples)
    val decibels = 20.0 * log10(max(rms, 1e-5))
    return ((decibels + 80.0) / 80.0).coerceIn(0.0, 1.0).pow(2.0).toFloat()
}
