package com.openclipboard

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OpenClipboardAppStateTest {
    @Test
    fun removeTrustedPeerFromState_updatesTrustedAndNearbyFlags() {
        // Save prior global state (OpenClipboardAppState is a singleton).
        val prevTrusted = OpenClipboardAppState.trustedPeers.toList()
        val prevNearby = OpenClipboardAppState.nearbyPeers.toList()

        try {
            OpenClipboardAppState.trustedPeers.clear()
            OpenClipboardAppState.nearbyPeers.clear()

            OpenClipboardAppState.trustedPeers.addAll(
                listOf(
                    TrustedPeerRecord(name = "Alice", peerId = "peer-a"),
                    TrustedPeerRecord(name = "Bob", peerId = "peer-b"),
                )
            )
            OpenClipboardAppState.nearbyPeers.addAll(
                listOf(
                    NearbyPeerRecord(peerId = "peer-a", name = "Alice", addr = "1.2.3.4:1", isTrusted = true),
                    NearbyPeerRecord(peerId = "peer-x", name = "X", addr = "1.2.3.4:2", isTrusted = false),
                )
            )

            OpenClipboardAppState.removeTrustedPeerFromState("peer-a")

            assertEquals(listOf("peer-b"), OpenClipboardAppState.trustedPeers.map { it.peerId })

            val a = OpenClipboardAppState.nearbyPeers.first { it.peerId == "peer-a" }
            assertFalse(a.isTrusted)

            val x = OpenClipboardAppState.nearbyPeers.first { it.peerId == "peer-x" }
            assertTrue(!x.isTrusted)
        } finally {
            OpenClipboardAppState.trustedPeers.clear()
            OpenClipboardAppState.trustedPeers.addAll(prevTrusted)
            OpenClipboardAppState.nearbyPeers.clear()
            OpenClipboardAppState.nearbyPeers.addAll(prevNearby)
        }
    }
}
