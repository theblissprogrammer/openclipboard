import XCTest
import OpenClipboardBindings

final class E2ETests: XCTestCase {
    final class Handler: EventHandler {
        let onClipboard: @Sendable (String, String) -> Void
        let onPeerConnected: @Sendable (String) -> Void
        let onError: @Sendable (String) -> Void

        init(
            onClipboard: @escaping @Sendable (String, String) -> Void,
            onPeerConnected: @escaping @Sendable (String) -> Void,
            onError: @escaping @Sendable (String) -> Void
        ) {
            self.onClipboard = onClipboard
            self.onPeerConnected = onPeerConnected
            self.onError = onError
        }

        func onClipboardText(peerId: String, text: String, tsMs: UInt64) {
            onClipboard(peerId, text)
        }

        func onFileReceived(peerId: String, name: String, dataPath: String) {}
        func onPeerConnected(peerId: String) { onPeerConnected(peerId) }
        func onPeerDisconnected(peerId: String) {}
        func onError(message: String) { onError(message) }
    }

    func testClipboardNodeLoopbackTextE2E() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("openclipboard-tests")
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)

        let idAPath = root.appendingPathComponent("a-identity.json").path
        let idBPath = root.appendingPathComponent("b-identity.json").path
        let trustAPath = root.appendingPathComponent("a-trust.json").path
        let trustBPath = root.appendingPathComponent("b-trust.json").path

        let idA = identityGenerate()
        let idB = identityGenerate()
        try idA.save(path: idAPath)
        try idB.save(path: idBPath)

        // Mutual trust so the handshake passes.
        let storeA = try trustStoreOpen(path: trustAPath)
        try storeA.add(peerId: idB.peerId(), identityPkB64: idB.pubkeyB64(), displayName: "B")

        let storeB = try trustStoreOpen(path: trustBPath)
        try storeB.add(peerId: idA.peerId(), identityPkB64: idA.pubkeyB64(), displayName: "A")

        let nodeA = try clipboardNodeNew(identityPath: idAPath, trustPath: trustAPath)
        let nodeB = try clipboardNodeNew(identityPath: idBPath, trustPath: trustBPath)

        let port = UInt16(Int.random(in: 20000...55000))

        let peerConnected = expectation(description: "peer connected")
        let receivedClipboard = expectation(description: "receive clipboard text")

        final class ErrorBox: @unchecked Sendable {
            private let lock = NSLock()
            private var _value: String?

            func set(_ v: String) {
                lock.lock(); defer { lock.unlock() }
                _value = v
            }

            func get() -> String? {
                lock.lock(); defer { lock.unlock() }
                return _value
            }
        }

        let errorBox = ErrorBox()

        let handler = Handler(
            onClipboard: { peerId, text in
                if !peerId.isEmpty && text == "hello" {
                    receivedClipboard.fulfill()
                }
            },
            onPeerConnected: { _ in
                peerConnected.fulfill()
            },
            onError: { message in
                errorBox.set(message)
                peerConnected.fulfill() // unblock waits so we can surface the actual error
                receivedClipboard.fulfill()
            }
        )

        try nodeA.startListener(port: port, handler: handler)

        // Give the listener a moment to bind on slower CI runners.
        Thread.sleep(forTimeInterval: 0.2)

        try nodeB.connectAndSendText(addr: "127.0.0.1:\(port)", text: "hello")

        // First ensure we actually got a connection/handshake.
        let r1 = XCTWaiter().wait(for: [peerConnected], timeout: 10.0)
        if let lastError = errorBox.get() {
            XCTFail("Node reported error during connect/handshake: \(lastError)")
        } else if r1 != .completed {
            XCTFail("Timed out waiting for peer connection")
        }

        // Then wait for clipboard payload.
        let r2 = XCTWaiter().wait(for: [receivedClipboard], timeout: 20.0)
        if let lastError = errorBox.get() {
            XCTFail("Node reported error: \(lastError)")
        } else if r2 != .completed {
            XCTFail("Timed out waiting for clipboard text")
        }

        nodeA.stop()
        nodeB.stop()
    }
}
