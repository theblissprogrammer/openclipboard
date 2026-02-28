package com.openclipboard

import org.junit.Assert.*
import org.junit.Test

class EchoSuppressorTest {
    @Test
    fun remoteWritesAreSuppressed() {
        val s = EchoSuppressor(capacity = 3)
        s.noteRemoteWrite("a")
        assertTrue(s.shouldIgnoreLocalChange("a"))
        assertFalse(s.shouldIgnoreLocalChange("b"))

        s.noteRemoteWrite("b")
        s.noteRemoteWrite("c")
        s.noteRemoteWrite("d")

        // capacity=3 -> "a" should be evicted
        assertFalse(s.shouldIgnoreLocalChange("a"))
        assertTrue(s.shouldIgnoreLocalChange("b"))
        assertTrue(s.shouldIgnoreLocalChange("c"))
        assertTrue(s.shouldIgnoreLocalChange("d"))
    }
}
