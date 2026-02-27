package com.openclipboard

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.openclipboard.EventHandler
import uniffi.openclipboard.clipboardNodeNew
import uniffi.openclipboard.identityGenerate
import uniffi.openclipboard.trustStoreOpen
import java.nio.file.Files
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class UniffiE2ETest {

    private class Handler(
        private val latch: CountDownLatch,
        private val expectedText: String,
    ) : EventHandler {
        var gotPeerId: String? = null
        var gotText: String? = null

        override fun onClipboardText(peerId: String, text: String, tsMs: ULong) {
            gotPeerId = peerId
            gotText = text
            if (text == expectedText) {
                latch.countDown()
            }
        }

        override fun onFileReceived(peerId: String, name: String, dataPath: String) {}
        override fun onPeerConnected(peerId: String) {}
        override fun onPeerDisconnected(peerId: String) {}
        override fun onError(message: String) {}
    }

    @Test
    fun clipboardNode_loopbackText_e2e() {
        val root = Files.createTempDirectory("openclipboard-android-test-" + UUID.randomUUID())

        val idAPath = root.resolve("a-identity.json").toString()
        val idBPath = root.resolve("b-identity.json").toString()
        val trustAPath = root.resolve("a-trust.json").toString()
        val trustBPath = root.resolve("b-trust.json").toString()

        val idA = identityGenerate()
        val idB = identityGenerate()
        idA.save(idAPath)
        idB.save(idBPath)

        // Mutual trust so handshake passes.
        val storeA = trustStoreOpen(trustAPath)
        storeA.add(idB.peerId(), idB.pubkeyB64(), "B")

        val storeB = trustStoreOpen(trustBPath)
        storeB.add(idA.peerId(), idA.pubkeyB64(), "A")

        val nodeA = clipboardNodeNew(idAPath, trustAPath)
        val nodeB = clipboardNodeNew(idBPath, trustBPath)

        val port = (20000..55000).random().toUShort()
        val latch = CountDownLatch(1)
        val handler = Handler(latch, expectedText = "hello")

        nodeA.startListener(port, handler)
        nodeB.connectAndSendText("127.0.0.1:$port", "hello")

        val ok = latch.await(5, TimeUnit.SECONDS)
        nodeA.stop()
        nodeB.stop()

        assertTrue("expected clipboard text callback", ok)
        assertEquals("hello", handler.gotText)
        assertTrue("expected non-empty peer id", !handler.gotPeerId.isNullOrEmpty())
    }

    @Test
    fun trustStore_roundtrip_add_get_list_remove() {
        val root = Files.createTempDirectory("openclipboard-android-test-" + UUID.randomUUID())
        val trustPath = root.resolve("trust.json").toString()

        val id = identityGenerate()

        val store = trustStoreOpen(trustPath)
        store.add(id.peerId(), id.pubkeyB64(), "Device")

        val got = store.get(id.peerId())
        assertTrue(got != null)
        assertEquals(id.peerId(), got!!.peerId)

        val list = store.list()
        assertTrue(list.isNotEmpty())

        val removed = store.remove(id.peerId())
        assertTrue(removed)
    }
}
