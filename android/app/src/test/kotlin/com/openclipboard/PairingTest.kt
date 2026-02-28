package com.openclipboard

import org.junit.Assert.assertEquals
import org.junit.Test

class PairingTest {

    @Test
    fun pairing_roundtrip_init_respond_finalize_derives_same_code() {
        val aPeerId = "peerA"
        val bPeerId = "peerB"

        // fake 32-byte pubkeys (base64)
        val aPk = Pairing.pkB64FromBytes(ByteArray(32) { 1 })
        val bPk = Pairing.pkB64FromBytes(ByteArray(32) { 2 })

        val nonce = ByteArray(32) { 7 }
        val init = Pairing.createInitPayload(
            myPeerId = aPeerId,
            myName = "Alice",
            myIdentityPkB64 = aPk,
            myLanPort = 18455,
            nonce = nonce,
        )

        val resp = Pairing.respondToInit(
            initQr = init.initQr,
            myPeerId = bPeerId,
            myName = "Bob",
            myIdentityPkB64 = bPk,
            myLanPort = 18455,
        )

        val fin = Pairing.finalize(init.initQr, resp.respQr)

        assertEquals(resp.confirmationCode, fin.confirmationCode)
    }

    @Test
    fun scanned_qr_string_with_whitespace_is_accepted() {
        val aPeerId = "peerA"
        val bPeerId = "peerB"

        val aPk = Pairing.pkB64FromBytes(ByteArray(32) { 1 })
        val bPk = Pairing.pkB64FromBytes(ByteArray(32) { 2 })

        val nonce = ByteArray(32) { 7 }
        val init = Pairing.createInitPayload(
            myPeerId = aPeerId,
            myName = "Alice",
            myIdentityPkB64 = aPk,
            myLanPort = 18455,
            nonce = nonce,
        )

        val scanned = "\n  ${init.initQr}\n\n"

        val resp = Pairing.respondToInit(
            initQr = scanned,
            myPeerId = bPeerId,
            myName = "Bob",
            myIdentityPkB64 = bPk,
            myLanPort = 18455,
        )

        val fin = Pairing.finalize(scanned, resp.respQr + "\n")
        assertEquals(resp.confirmationCode, fin.confirmationCode)
    }
}
