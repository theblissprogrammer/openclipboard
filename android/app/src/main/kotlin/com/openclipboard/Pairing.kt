package com.openclipboard

import android.util.Base64
import uniffi.openclipboard.PairingPayload
import uniffi.openclipboard.OpenClipboardError
import uniffi.openclipboard.pairingPayloadCreate
import uniffi.openclipboard.pairingPayloadFromQrString
import uniffi.openclipboard.deriveConfirmationCode
import java.security.SecureRandom

/**
 * Pairing helper functions.
 *
 * MVP design:
 * - Initiator generates an init payload (QR string)
 * - Responder parses init QR, generates resp payload (QR string)
 * - Both derive the same 6-digit confirmation code
 * - Once confirmed, each side writes the other peer into its TrustStore
 */
object Pairing {
    private val rng = SecureRandom()

    fun randomNonce32(): ByteArray = ByteArray(32).also { rng.nextBytes(it) }

    fun pkBytesFromB64(b64: String): ByteArray = Base64.decode(b64, Base64.DEFAULT)

    fun pkB64FromBytes(bytes: ByteArray): String = Base64.encodeToString(bytes, Base64.NO_WRAP)

    data class InitResult(
        val initQr: String,
        val nonce: ByteArray,
    )

    fun createInitPayload(
        myPeerId: String,
        myName: String,
        myIdentityPkB64: String,
        myLanPort: Int,
        nonce: ByteArray = randomNonce32(),
    ): InitResult {
        val payload = pairingPayloadCreate(
            version = 1.toUByte(),
            peerId = myPeerId,
            name = myName,
            identityPk = pkBytesFromB64(myIdentityPkB64).map { it.toUByte() },
            lanPort = myLanPort.toUShort(),
            nonce = nonce.map { it.toUByte() },
        )
        val qr = payload.toQrString()
        return InitResult(initQr = qr, nonce = nonce)
    }

    data class RespondResult(
        val respQr: String,
        val confirmationCode: String,
        val init: PairingPayload,
        val resp: PairingPayload,
    )

    @Throws(OpenClipboardError::class)
    fun respondToInit(
        initQr: String,
        myPeerId: String,
        myName: String,
        myIdentityPkB64: String,
        myLanPort: Int,
    ): RespondResult {
        val init = pairingPayloadFromQrString(initQr)
        val resp = pairingPayloadCreate(
            version = 1.toUByte(),
            peerId = myPeerId,
            name = myName,
            identityPk = pkBytesFromB64(myIdentityPkB64).map { it.toUByte() },
            lanPort = myLanPort.toUShort(),
            nonce = init.nonce(),
        )
        val respQr = resp.toQrString()
        val code = deriveConfirmationCode(
            nonce = init.nonce(),
            peerAId = init.peerId(),
            peerBId = resp.peerId(),
        )
        return RespondResult(respQr = respQr, confirmationCode = code, init = init, resp = resp)
    }

    data class FinalizeResult(
        val confirmationCode: String,
        val init: PairingPayload,
        val resp: PairingPayload,
    )

    @Throws(OpenClipboardError::class)
    fun finalize(initQr: String, respQr: String): FinalizeResult {
        val init = pairingPayloadFromQrString(initQr)
        val resp = pairingPayloadFromQrString(respQr)

        val initNonce = init.nonce()
        val respNonce = resp.nonce()
        require(initNonce == respNonce) { "nonce mismatch" }

        val code = deriveConfirmationCode(
            nonce = initNonce,
            peerAId = init.peerId(),
            peerBId = resp.peerId(),
        )
        return FinalizeResult(confirmationCode = code, init = init, resp = resp)
    }
}
