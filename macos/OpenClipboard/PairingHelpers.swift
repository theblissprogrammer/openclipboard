import Foundation
import OpenClipboardBindings

/// Small pairing helpers shared by UI and tests.
enum PairingHelpers {
    static func normalizeQrString(_ s: String) -> String {
        s.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    struct FinalizeInfo {
        var code: String
        var remotePeerId: String
        var remoteName: String
        var remotePkB64: String
    }

    static func finalizePairing(initQr: String, respQr: String) throws -> FinalizeInfo {
        let initPayload = try pairingPayloadFromQrString(s: normalizeQrString(initQr))
        let respPayload = try pairingPayloadFromQrString(s: normalizeQrString(respQr))

        if initPayload.nonce() != respPayload.nonce() {
            throw NSError(domain: "OpenClipboard", code: 1, userInfo: [NSLocalizedDescriptionKey: "nonce mismatch"])
        }

        let code = deriveConfirmationCode(
            nonce: initPayload.nonce(),
            peerAId: initPayload.peerId(),
            peerBId: respPayload.peerId()
        )
        let remotePkB64 = Data(respPayload.identityPk()).base64EncodedString()

        return FinalizeInfo(
            code: code,
            remotePeerId: respPayload.peerId(),
            remoteName: respPayload.name(),
            remotePkB64: remotePkB64
        )
    }
}
