import XCTest
import OpenClipboardBindings

final class E2ETests: XCTestCase {
    final class Handler: EventHandler {
        let onClipboard: @Sendable (String, String) -> Void
        let onError: @Sendable (String) -> Void

        init(
            onClipboard: @escaping @Sendable (String, String) -> Void,
            onError: @escaping @Sendable (String) -> Void
        ) {
            self.onClipboard = onClipboard
            self.onError = onError
        }

        func onClipboardText(peerId: String, text: String, tsMs: UInt64) {
            onClipboard(peerId, text)
        }

        func onFileReceived(peerId: String, name: String, dataPath: String) {}
        func onPeerConnected(peerId: String) {}
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

        let exp = expectation(description: "receive clipboard text")

        let lock = NSLock()
        var lastError: String?

        let handler = Handler(
            onClipboard: { peerId, text in
                if !peerId.isEmpty && text == "hello" {
                    exp.fulfill()
                }
            },
            onError: { message in
                lock.lock(); defer { lock.unlock() }
                lastError = message
            }
        )

        try nodeA.startListener(port: port, handler: handler)

        // Give the listener a moment to bind on slower CI runners.
        Thread.sleep(forTimeInterval: 0.2)

        try nodeB.connectAndSendText(addr: "127.0.0.1:\(port)", text: "hello")

        let result = XCTWaiter().wait(for: [exp], timeout: 20.0)
        if result != .completed {
            lock.lock(); defer { lock.unlock() }
            if let lastError {
                XCTFail("Timed out waiting for clipboard text; last node error: \(lastError)")
            } else {
                XCTFail("Timed out waiting for clipboard text; no error was reported by node")
            }
        }

        nodeA.stop()
        nodeB.stop()
    }
}
