package com.openclipboard

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files

class OpenClipboardAppStateResetTest {

    @Test
    fun resetIdentity_deletesOnlyIdentity() {
        val dir = Files.createTempDirectory("oc-reset-").toFile()
        val identity = File(dir, "identity.json").apply { writeText("id") }
        val trust = File(dir, "trust.json").apply { writeText("trust") }

        val res = OpenClipboardAppState.resetFiles(
            identityFile = identity,
            trustFile = trust,
            resetIdentity = true,
            resetTrust = false,
        )

        assertTrue(res.identityDeleted)
        assertFalse(res.trustDeleted)
        assertFalse(identity.exists())
        assertTrue(trust.exists())
    }

    @Test
    fun clearTrustedPeers_deletesOnlyTrust() {
        val dir = Files.createTempDirectory("oc-reset-").toFile()
        val identity = File(dir, "identity.json").apply { writeText("id") }
        val trust = File(dir, "trust.json").apply { writeText("trust") }

        val res = OpenClipboardAppState.resetFiles(
            identityFile = identity,
            trustFile = trust,
            resetIdentity = false,
            resetTrust = true,
        )

        assertFalse(res.identityDeleted)
        assertTrue(res.trustDeleted)
        assertTrue(identity.exists())
        assertFalse(trust.exists())
    }

    @Test
    fun resetAll_deletesBoth() {
        val dir = Files.createTempDirectory("oc-reset-").toFile()
        val identity = File(dir, "identity.json").apply { writeText("id") }
        val trust = File(dir, "trust.json").apply { writeText("trust") }

        val res = OpenClipboardAppState.resetFiles(
            identityFile = identity,
            trustFile = trust,
            resetIdentity = true,
            resetTrust = true,
        )

        assertTrue(res.identityDeleted)
        assertTrue(res.trustDeleted)
        assertFalse(identity.exists())
        assertFalse(trust.exists())
    }

    @Test
    fun resetFiles_whenMissing_isNoop() {
        val dir = Files.createTempDirectory("oc-reset-").toFile()
        val identity = File(dir, "identity.json")
        val trust = File(dir, "trust.json")

        val res = OpenClipboardAppState.resetFiles(
            identityFile = identity,
            trustFile = trust,
            resetIdentity = true,
            resetTrust = true,
        )

        assertFalse(res.identityDeleted)
        assertFalse(res.trustDeleted)
        assertFalse(identity.exists())
        assertFalse(trust.exists())
    }
}
