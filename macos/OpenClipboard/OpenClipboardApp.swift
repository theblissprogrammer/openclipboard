import Cocoa
import SwiftUI
import Security
import OpenClipboardBindings

@main
struct OpenClipboardApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        Settings {
            SettingsView()
        }
    }
}

final class MacEventHandler: EventHandler {
    private let onEvent: @Sendable (Event) -> Void

    enum Event {
        case clipboardText(peerId: String, text: String, tsMs: UInt64)
        case fileReceived(peerId: String, name: String, dataPath: String)
        case peerConnected(peerId: String)
        case peerDisconnected(peerId: String)
        case error(message: String)
    }

    init(onEvent: @escaping @Sendable (Event) -> Void) {
        self.onEvent = onEvent
    }

    func onClipboardText(peerId: String, text: String, tsMs: UInt64) {
        onEvent(.clipboardText(peerId: peerId, text: text, tsMs: tsMs))
    }

    func onFileReceived(peerId: String, name: String, dataPath: String) {
        onEvent(.fileReceived(peerId: peerId, name: name, dataPath: dataPath))
    }

    func onPeerConnected(peerId: String) {
        onEvent(.peerConnected(peerId: peerId))
    }

    func onPeerDisconnected(peerId: String) {
        onEvent(.peerDisconnected(peerId: peerId))
    }

    func onError(message: String) {
        onEvent(.error(message: message))
    }
}

final class MacDiscoveryHandler: DiscoveryHandler {
    private let onEvent: @Sendable (Event) -> Void

    enum Event {
        case peerDiscovered(peerId: String, name: String, addr: String)
        case peerLost(peerId: String)
    }

    init(onEvent: @escaping @Sendable (Event) -> Void) {
        self.onEvent = onEvent
    }

    func onPeerDiscovered(peerId: String, name: String, addr: String) {
        onEvent(.peerDiscovered(peerId: peerId, name: name, addr: addr))
    }

    func onPeerLost(peerId: String) {
        onEvent(.peerLost(peerId: peerId))
    }
}

@MainActor
class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem?
    private var statusBarMenu: NSMenu?

    private var node: ClipboardNode?
    private var handler: MacEventHandler?
    private var discoveryHandler: MacDiscoveryHandler?

    private var connectedPeers: [String] = []

    // Phase 3: local clipboard monitoring + echo suppression.
    private var pasteboardChangeCount: Int = NSPasteboard.general.changeCount
    private var pasteboardTimer: Timer?
    private var recentRemoteWrites: [String] = []
    private let recentRemoteWritesCap: Int = 20

    struct NearbyPeer {
        var peerId: String
        var name: String
        var addr: String
    }
    private var nearbyPeers: [String: NearbyPeer] = [:]

    private var listenerPort: UInt16 = 18455

    func applicationDidFinishLaunching(_ notification: Notification) {
        setupMenuBarApp()
        wireUpFFI()
    }

    private func wireUpFFI() {
        do {
            let identityPath = defaultIdentityPath()
            let trustPath = trustStoreDefaultPath()
            let node = try clipboardNodeNew(identityPath: identityPath, trustPath: trustPath)
            self.node = node

            self.handler = MacEventHandler { [weak self] event in
                guard let self else { return }
                Task { @MainActor in
                    self.handle(event)
                }
            }

            if let handler = self.handler {
                let name = Host.current().localizedName ?? "macOS"
                try node.startSync(port: listenerPort, deviceName: name, handler: handler)
                startPasteboardMonitor()
            }

            updateMenu()
        } catch {
            showError("FFI init failed: \(error)")
        }
    }

    private func handle(_ event: MacEventHandler.Event) {
        switch event {
        case let .peerConnected(peerId):
            if !connectedPeers.contains(peerId) {
                connectedPeers.append(peerId)
            }
            updateMenu()

        case let .peerDisconnected(peerId):
            connectedPeers.removeAll { $0 == peerId }
            updateMenu()

        case let .clipboardText(peerId, text, _):
            // Put received text on the system clipboard (MVP behavior).
            noteRemoteWrite(text)
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)

            // Also show a small notification.
            let n = NSUserNotification()
            n.title = "Clipboard received"
            n.informativeText = "From \(peerId)"
            NSUserNotificationCenter.default.deliver(n)

        case let .fileReceived(peerId, name, dataPath):
            let n = NSUserNotification()
            n.title = "File received"
            n.informativeText = "\(name) from \(peerId) saved to \(dataPath)"
            NSUserNotificationCenter.default.deliver(n)

        case let .error(message):
            showError(message)
        }
    }


    private func noteRemoteWrite(_ text: String) {
        if recentRemoteWrites.last == text { return }
        recentRemoteWrites.append(text)
        if recentRemoteWrites.count > recentRemoteWritesCap {
            recentRemoteWrites.removeFirst(recentRemoteWrites.count - recentRemoteWritesCap)
        }
    }

    private func shouldIgnoreLocalChange(_ text: String) -> Bool {
        return recentRemoteWrites.contains(text)
    }

    private func startPasteboardMonitor() {
        pasteboardTimer?.invalidate()
        pasteboardChangeCount = NSPasteboard.general.changeCount
        pasteboardTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            guard let self else { return }
            let pb = NSPasteboard.general
            let cc = pb.changeCount
            if cc == self.pasteboardChangeCount { return }
            self.pasteboardChangeCount = cc
            guard let text = pb.string(forType: .string) else { return }
            if self.shouldIgnoreLocalChange(text) { return }
            do {
                try self.node?.sendClipboardText(text: text)
            } catch {
                self.showError("Broadcast failed: \(error)")
            }
        }
    }
    private func setupMenuBarApp() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem?.button?.title = "📋"

        statusBarMenu = NSMenu()
        statusItem?.menu = statusBarMenu

        updateMenu()
    }

    private func updateMenu() {
        guard let menu = statusBarMenu else { return }
        menu.removeAllItems()

        let peerIdValue = node?.peerId() ?? "(initializing…)"
        let peerIdItem = NSMenuItem(title: "Peer ID: \(peerIdValue)", action: nil, keyEquivalent: "")
        peerIdItem.isEnabled = false
        menu.addItem(peerIdItem)

        let portItem = NSMenuItem(title: "Listening: \(listenerPort)", action: nil, keyEquivalent: "")
        portItem.isEnabled = false
        menu.addItem(portItem)

        menu.addItem(NSMenuItem.separator())

        menu.addItem(NSMenuItem(title: "Connected Peers", action: nil, keyEquivalent: ""))
        if connectedPeers.isEmpty {
            let noPeersItem = NSMenuItem(title: "  None", action: nil, keyEquivalent: "")
            noPeersItem.isEnabled = false
            menu.addItem(noPeersItem)
        } else {
            for peer in connectedPeers {
                let peerItem = NSMenuItem(title: "  \(peer)", action: nil, keyEquivalent: "")
                peerItem.isEnabled = false
                menu.addItem(peerItem)
            }
        }

        menu.addItem(NSMenuItem.separator())

        menu.addItem(NSMenuItem(title: "Nearby Devices", action: nil, keyEquivalent: ""))
        let nearbyList = nearbyPeers.values.sorted { $0.name < $1.name }
        if nearbyList.isEmpty {
            let item = NSMenuItem(title: "  None", action: nil, keyEquivalent: "")
            item.isEnabled = false
            menu.addItem(item)
        } else {
            for p in nearbyList {
                let trusted = isTrustedPeer(peerId: p.peerId)
                let title = "  \(p.name) — \(p.peerId.prefix(8))…" + (trusted ? "" : " (unpaired)")
                let item = NSMenuItem(title: title, action: #selector(pairWithNearby(_:)), keyEquivalent: "")
                item.target = self
                item.representedObject = p
                item.isEnabled = !trusted
                menu.addItem(item)
            }
        }

        menu.addItem(NSMenuItem.separator())

        menu.addItem(NSMenuItem(title: "Pair…", action: #selector(pairGeneric), keyEquivalent: "p"))
        menu.addItem(NSMenuItem(title: "Send Clipboard…", action: #selector(sendClipboard), keyEquivalent: "s"))
        menu.addItem(NSMenuItem(title: "Settings", action: #selector(showSettings), keyEquivalent: ","))

        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q"))
    }

    private func isTrustedPeer(peerId: String) -> Bool {
        do {
            let store = try trustStoreOpen(path: trustStoreDefaultPath())
            return try store.get(peerId: peerId) != nil
        } catch {
            return false
        }
    }

    @objc private func pairWithNearby(_ sender: NSMenuItem) {
        guard let p = sender.representedObject as? NearbyPeer else {
            return
        }
        runPairFlow(defaultPeerName: p.name)
    }

    @objc private func pairGeneric() {
        runPairFlow(defaultPeerName: nil)
    }

    private func runPairFlow(defaultPeerName: String?) {
        let identityPath = defaultIdentityPath()

        guard let myId = try? identityLoad(path: identityPath) else {
            showError("Failed to load identity")
            return
        }

        let myPeerId = myId.peerId()
        let myPk = myId.pubkeyB64()
        let myName = Host.current().localizedName ?? "macOS"

        let role = NSAlert()
        role.messageText = "Pair Device"
        role.informativeText = defaultPeerName == nil ? "Choose a role" : "Nearby: \(defaultPeerName!)\n\nChoose a role"
        role.addButton(withTitle: "Initiate")
        role.addButton(withTitle: "Respond")
        role.addButton(withTitle: "Cancel")
        let r = role.runModal()
        if r == .alertThirdButtonReturn { return }

        do {
            if r == .alertFirstButtonReturn {
                // Initiator
                let initPayload = pairingPayloadCreate(
                    version: 1,
                    peerId: myPeerId,
                    name: myName,
                    identityPk: Data(base64Encoded: myPk)!.map { $0 },
                    lanPort: UInt16(listenerPort),
                    nonce: randomNonce32()
                )
                let initQr = initPayload.toQrString()

                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(initQr, forType: .string)

                let showInit = NSAlert()
                showInit.messageText = "Init string copied"
                showInit.informativeText = "Paste this init string on the other device.\n\n(init is already copied to your clipboard)"
                showInit.addButton(withTitle: "Continue")
                showInit.runModal()

                let input = NSAlert()
                input.messageText = "Paste response string"
                let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 420, height: 24))
                field.placeholderString = "openclipboard://pair?..."
                input.accessoryView = field
                input.addButton(withTitle: "Next")
                input.addButton(withTitle: "Cancel")
                if input.runModal() != .alertFirstButtonReturn { return }

                let respQr = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
                let fin = try finalizePairing(initQr: initQr, respQr: respQr)

                let confirm = NSAlert()
                confirm.messageText = "Confirmation code: \(fin.code)"
                confirm.informativeText = "Confirm the code matches on the other device, then click Confirm."
                confirm.addButton(withTitle: "Confirm")
                confirm.addButton(withTitle: "Cancel")
                if confirm.runModal() != .alertFirstButtonReturn { return }

                let store = try trustStoreOpen(path: trustStoreDefaultPath())
                try store.add(peerId: fin.remotePeerId, identityPkB64: fin.remotePkB64, displayName: fin.remoteName)
                showError("Paired with \(fin.remotePeerId)")
            } else {
                // Responder
                let input = NSAlert()
                input.messageText = "Paste init string"
                let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 420, height: 24))
                field.placeholderString = "openclipboard://pair?..."
                input.accessoryView = field
                input.addButton(withTitle: "Next")
                input.addButton(withTitle: "Cancel")
                if input.runModal() != .alertFirstButtonReturn { return }

                let initQr = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
                let initPayload = try pairingPayloadFromQrString(s: initQr)

                let respPayload = pairingPayloadCreate(
                    version: 1,
                    peerId: myPeerId,
                    name: myName,
                    identityPk: Data(base64Encoded: myPk)!.map { $0 },
                    lanPort: UInt16(listenerPort),
                    nonce: initPayload.nonce()
                )
                let respQr = respPayload.toQrString()

                let code = deriveConfirmationCode(nonce: initPayload.nonce(), peerAId: initPayload.peerId(), peerBId: respPayload.peerId())

                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(respQr, forType: .string)

                let showResp = NSAlert()
                showResp.messageText = "Response copied"
                showResp.informativeText = "Send this response to the initiator.\n\nConfirmation code: \(code)\n\n(response is copied to clipboard)"
                showResp.addButton(withTitle: "Confirm")
                showResp.addButton(withTitle: "Cancel")
                if showResp.runModal() != .alertFirstButtonReturn { return }

                let store = try trustStoreOpen(path: trustStoreDefaultPath())
                let remotePkB64 = Data(initPayload.identityPk()).base64EncodedString()
                try store.add(peerId: initPayload.peerId(), identityPkB64: remotePkB64, displayName: initPayload.name())
                showError("Paired with \(initPayload.peerId())")
            }

            updateMenu()
        } catch {
            showError("Pairing failed: \(error)")
        }
    }

    private func randomNonce32() -> [UInt8] {
        var bytes = [UInt8](repeating: 0, count: 32)
        let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        if status != errSecSuccess {
            // Fallback
            for i in 0..<bytes.count { bytes[i] = UInt8.random(in: 0...255) }
        }
        return bytes
    }

    private struct FinalizeInfo {
        var code: String
        var remotePeerId: String
        var remoteName: String
        var remotePkB64: String
    }

    private func finalizePairing(initQr: String, respQr: String) throws -> FinalizeInfo {
        let init = try pairingPayloadFromQrString(s: initQr)
        let resp = try pairingPayloadFromQrString(s: respQr)

        if init.nonce() != resp.nonce() {
            throw NSError(domain: "OpenClipboard", code: 1, userInfo: [NSLocalizedDescriptionKey: "nonce mismatch"])
        }

        let code = deriveConfirmationCode(nonce: init.nonce(), peerAId: init.peerId(), peerBId: resp.peerId())
        let remotePkB64 = Data(resp.identityPk()).base64EncodedString()
        return FinalizeInfo(code: code, remotePeerId: resp.peerId(), remoteName: resp.name(), remotePkB64: remotePkB64)
    }

    @objc private func sendClipboard() {
        guard let node else {
            showError("Clipboard node not initialized")
            return
        }

        let pb = NSPasteboard.general
        guard let text = pb.string(forType: .string), !text.isEmpty else {
            showError("Clipboard is empty or not text")
            return
        }

        let alert = NSAlert()
        alert.messageText = "Send Clipboard"
        alert.informativeText = "Enter peer address (ip:port)"

        let input = NSTextField(frame: NSRect(x: 0, y: 0, width: 320, height: 24))
        input.placeholderString = "192.168.1.10:18455"
        alert.accessoryView = input

        alert.addButton(withTitle: "Send")
        alert.addButton(withTitle: "Cancel")

        let res = alert.runModal()
        if res != .alertFirstButtonReturn {
            return
        }

        let addr = input.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !addr.isEmpty else { return }

        do {
            try node.connectAndSendText(addr: addr, text: text)
        } catch {
            showError("Send failed: \(error)")
        }
    }

    @objc private func showSettings() {
        NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)
    }

    @objc private func quit() {
        node?.stop()
        NSApplication.shared.terminate(self)
    }

    private func showError(_ message: String) {
        // Avoid spamming alerts if the app is in background; use notification.
        let n = NSUserNotification()
        n.title = "OpenClipboard"
        n.informativeText = message
        NSUserNotificationCenter.default.deliver(n)
    }
}
