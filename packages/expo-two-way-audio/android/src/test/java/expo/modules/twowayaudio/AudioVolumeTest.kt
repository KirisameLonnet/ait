package expo.modules.twowayaudio

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AudioVolumeTest {
    @Test
    fun positiveAndNegativeSamplesHaveEqualVolume() {
        // +16383 has a negative low byte in Kotlin; -16383 has a positive one.
        val positive = pcm16VolumeLevel(byteArrayOf(0xff.toByte(), 0x3f))
        val negative = pcm16VolumeLevel(byteArrayOf(0x01, 0xc0.toByte()))
        assertEquals(negative, positive, 0.00001f)
        assertEquals(0.85515f, positive, 0.0001f)
    }

    @Test
    fun fullScaleSamplesStayAtTheTopOfTheMeter() {
        assertTrue(pcm16VolumeLevel(byteArrayOf(0xff.toByte(), 0x7f)) > 0.999f)
        assertEquals(1.0f, pcm16VolumeLevel(byteArrayOf(0x00, 0x80.toByte())), 0.00001f)
    }

    @Test
    fun silenceAndIncompleteSamplesDoNotProduceNan() {
        assertEquals(0.0f, pcm16VolumeLevel(byteArrayOf()), 0.0f)
        assertEquals(0.0f, pcm16VolumeLevel(byteArrayOf(0xff.toByte())), 0.0f)
        assertEquals(0.0f, pcm16VolumeLevel(byteArrayOf(0x00, 0x00)), 0.0f)
    }

    @Test
    fun completeSamplesDetermineTheRmsAcrossAChunk() {
        // Alternating full-scale negative samples and silence has RMS sqrt(1/2).
        val chunk = byteArrayOf(0x00, 0x80.toByte(), 0x00, 0x00, 0xff.toByte())
        assertEquals(0.92616f, pcm16VolumeLevel(chunk), 0.0001f)
    }
}
