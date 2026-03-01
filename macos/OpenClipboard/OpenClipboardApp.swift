import Cocoa
import SwiftUI
import Security
import UserNotifications
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

// MARK: - Real ClipboardProvider using NSPasteboard

final class MacClipboardProvider: ClipboardCallback, @unchecked Sendable {
    func readText() -> String? {
        return NSPasteboard.general.string(forType: .string)
    }

    func writeText(text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}

// MARK: - Event Handler

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

// MARK: - Discovery Handler

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

// MARK: - App Delegate

@MainActor
class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem?
    private var statusBarMenu: NSMenu?

    private var node: ClipboardNode?
    private var handler: MacEventHandler?
    private var discoveryHandler: MacDiscoveryHandler?
    private var clipboardProvider: MacClipboardProvider?

    private var connectedPeers: [String] = []

    struct NearbyPeer {
        var peerId: String
        var name: String
        var addr: String
    }
    private var nearbyPeers: [String: NearbyPeer] = [:]

    private var listenerPort: UInt16 = 18455

    private var syncEnabled: Bool = true

    private var pairingQRWindowController: PairingQRWindowController?

    // History size limit (stored in UserDefaults)
    private var historySizeLimit: UInt32 {
        let val = UserDefaults.standard.integer(forKey: "historySizeLimit")
        return val > 0 ? UInt32(val) : 50
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        setupMenuBarApp()
        requestNotificationAuthorizationIfNeeded()
        wireUpFFI()
    }

    // MARK: - FFI Setup

    private func wireUpFFI() {
        if node != nil { return }
        syncEnabled = true
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

            let provider = MacClipboardProvider()
            self.clipboardProvider = provider

            if let handler = self.handler {
                let name = Host.current().localizedName ?? "macOS"
                // Use start_mesh instead of start_sync — handles clipboard polling + broadcast automatically
                try node.startMesh(
                    port: listenerPort,
                    deviceName: name,
                    handler: handler,
                    provider: provider,
                    pollIntervalMs: 250
                )
            }

            self.discoveryHandler = MacDiscoveryHandler { [weak self] event in
                guard let self else { return }
                Task { @MainActor in
                    switch event {
                    case let .peerDiscovered(peerId, name, addr):
                        self.nearbyPeers[peerId] = NearbyPeer(peerId: peerId, name: name, addr: addr)
                        self.updateMenu()
                    case let .peerLost(peerId):
                        self.nearbyPeers.removeValue(forKey: peerId)
                        self.updateMenu()
                    }
                }
            }

            if let discoveryHandler = self.discoveryHandler {
                let name = Host.current().localizedName ?? "macOS"
                try node.startDiscovery(deviceName: name, handler: discoveryHandler)
            }

            updateMenu()
        } catch {
            showError("FFI init failed: \(error)")
        }
    }

    // MARK: - Event Handling

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
            // Mesh handles writing to clipboard via ClipboardCallback.
            // Just show a notification and refresh history menu.
            let preview = text.count > 50 ? String(text.prefix(50)) + "…" : text
            postUserNotification(title: "Clipboard received", body: "From \(peerId): \(preview)")
            updateMenu()

        case let .fileReceived(peerId, name, dataPath):
            postUserNotification(title: "File received", body: "\(name) from \(peerId) saved to \(dataPath)")

        case let .error(message):
            showError(message)
        }
    }

    // MARK: - Menu Bar

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

        let syncTitle = syncEnabled ? "Sync: On" : "Sync: Off"
        let syncItem = NSMenuItem(title: syncTitle, action: #selector(toggleSync), keyEquivalent: "t")
        syncItem.target = self
        menu.addItem(syncItem)

        menu.addItem(NSMenuItem.separator())

        // Connected Peers
        menu.addItem(NSMenuItem(title: "Connected Peers", action: nil, keyEquivalent: ""))
        if connectedPeers.isEmpty {
            let noPeersItem = NSMenuItem(title: "  None", action: nil, keyEquivalent: "")
            noPeersItem.isEnabled = false
            menu.addItem(noPeersItem)
        } else {
            for peer in connectedPeers {
                let isOnline = true // connected peers are online by definition
                let status = isOnline ? "🟢" : "⚪"
                let peerItem = NSMenuItem(title: "  \(status) \(peer)", action: nil, keyEquivalent: "")
                peerItem.isEnabled = false
                menu.addItem(peerItem)
            }
        }

        menu.addItem(NSMenuItem.separator())

        // Clipboard History Submenu
        let historyMenuItem = NSMenuItem(title: "Clipboard History", action: nil, keyEquivalent: "")
        let historyMenu = NSMenu(title: "Clipboard History")
        historyMenuItem.submenu = historyMenu
        buildHistorySubmenu(historyMenu)
        menu.addItem(historyMenuItem)

        menu.addItem(NSMenuItem.separator())

        // Nearby Devices
        menu.addItem(NSMenuItem(title: "Nearby Devices", action: nil, keyEquivalent: ""))
        let nearbyList = nearbyPeers.values.sorted { $0.name < $1.name }
        if nearbyList.isEmpty {
            let item = NSMenuItem(title: "  None", action: nil, keyEquivalent: "")
            item.isEnabled = false
            menu.addItem(item)
        } else {
            for p in nearbyList {
                let trusted = isTrustedPeer(peerId: p.peerId)
                let online = connectedPeers.contains(p.peerId)
                let status = online ? "🟢" : (trusted ? "⚪" : "")
                let title = "  \(status) \(p.name) — \(p.peerId.prefix(8))…" + (trusted ? "" : " (unpaired)")
                let item = NSMenuItem(title: title, action: #selector(pairWithNearby(_:)), keyEquivalent: "")
                item.target = self
                item.representedObject = p
                item.isEnabled = !trusted
                menu.addItem(item)
            }
        }

        menu.addItem(NSMenuItem.separator())

        // Pairing
        let pairingMenuItem = NSMenuItem(title: "Pairing", action: nil, keyEquivalent: "")
        let pairingMenu = NSMenu(title: "Pairing")
        pairingMenuItem.submenu = pairingMenu
        pairingMenu.addItem(NSMenuItem(title: "Show Pairing QR…", action: #selector(showPairingQR), keyEquivalent: ""))
        pairingMenu.items.last?.target = self
        pairingMenu.addItem(NSMenuItem(title: "Pair…", action: #selector(pairGeneric), keyEquivalent: "p"))
        pairingMenu.items.last?.target = self
        menu.addItem(pairingMenuItem)

        menu.addItem(NSMenuItem(title: "Send Clipboard…", action: #selector(sendClipboard), keyEquivalent: "s"))
        menu.addItem(NSMenuItem(title: "Settings", action: #selector(showSettings), keyEquivalent: ","))

        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q"))
    }

    // MARK: - History Submenu

    private func buildHistorySubmenu(_ menu: NSMenu) {
        guard let node else {
            let item = NSMenuItem(title: "Not running", action: nil, keyEquivalent: "")
            item.isEnabled = false
            menu.addItem(item)
            return
        }

        let entries = node.getClipboardHistory(limit: historySizeLimit)

        if entries.isEmpty {
            let item = NSMenuItem(title: "No history", action: nil, keyEquivalent: "")
            item.isEnabled = false
            menu.addItem(item)
            return
        }

        // Show "All" entries
        let allItem = NSMenuItem(title: "All Devices", action: nil, keyEquivalent: "")
        allItem.isEnabled = false
        menu.addItem(allItem)
        menu.addItem(NSMenuItem.separator())

        for entry in entries.prefix(20) {
            let item = makeHistoryMenuItem(entry)
            menu.addItem(item)
        }

        // Group by device — add submenus per peer
        let peerNames = Set(entries.map { $0.sourcePeer })
        if peerNames.count > 1 {
            menu.addItem(NSMenuItem.separator())
            let byDeviceItem = NSMenuItem(title: "By Device", action: nil, keyEquivalent: "")
            byDeviceItem.isEnabled = false
            menu.addItem(byDeviceItem)

            for peerName in peerNames.sorted() {
                let peerItem = NSMenuItem(title: peerName, action: nil, keyEquivalent: "")
                let peerMenu = NSMenu(title: peerName)
                peerItem.submenu = peerMenu

                let peerEntries = node.getClipboardHistoryForPeer(peerName: peerName, limit: historySizeLimit)
                for entry in peerEntries.prefix(20) {
                    peerMenu.addItem(makeHistoryMenuItem(entry))
                }
                menu.addItem(peerItem)
            }
        }
    }

    private func makeHistoryMenuItem(_ entry: ClipboardHistoryEntry) -> NSMenuItem {
        let preview = entry.content.count > 60 ? String(entry.content.prefix(60)) + "…" : entry.content
        // Replace newlines with spaces for menu display
        let cleanPreview = preview.replacingOccurrences(of: "\n", with: " ")
        let timeAgo = relativeTimeString(timestampMs: entry.timestamp)
        let title = "  \(cleanPreview)  (\(entry.sourcePeer), \(timeAgo))"

        let item = NSMenuItem(title: title, action: #selector(recallHistoryEntry(_:)), keyEquivalent: "")
        item.target = self
        item.representedObject = entry.id
        return item
    }

    @objc private func recallHistoryEntry(_ sender: NSMenuItem) {
        guard let entryId = sender.representedObject as? String, let node else { return }
        do {
            _ = try node.recallFromHistory(entryId: entryId)
        } catch {
            showError("Failed to recall: \(error)")
        }
    }

    private func relativeTimeString(timestampMs: UInt64) -> String {
        let seconds = (UInt64(Date().timeIntervalSince1970 * 1000) - timestampMs) / 1000
        if seconds < 60 { return "just now" }
        if seconds < 3600 { return "\(seconds / 60)m ago" }
        if seconds < 86400 { return "\(seconds / 3600)h ago" }
        return "\(seconds / 86400)d ago"
    }

    // MARK: - Trust

    private func isTrustedPeer(peerId: String) -> Bool {
        do {
            let store = try trustStoreOpen(path: trustStoreDefaultPath())
            return try store.get(peerId: peerId) != nil
        } catch {
            return false
        }
    }

    // MARK: - Pairing

    @objc private func pairWithNearby(_ sender: NSMenuItem) {
        guard let p = sender.representedObject as? NearbyPeer else {
            return
        }
        runPairFlow(defaultPeerName: p.name)
    }

    @objc private func pairGeneric() {
        runPairFlow(defaultPeerName: nil)
    }

    @objc private func showPairingQR() {
        let identityPath = defaultIdentityPath()

        guard let myId = try? identityLoad(path: identityPath) else {
            showError("Failed to load identity")
            return
        }

        let myPeerId = myId.peerId()
        let myPk = myId.pubkeyB64()
        let myName = Host.current().localizedName ?? "macOS"
        let lanAddrs = getLanAddresses()

        do {
            let initPayload = pairingPayloadCreate(
                version: 1,
                peerId: myPeerId,
                name: myName,
                identityPk: Data(base64Encoded: myPk)!.map { $0 },
                lanPort: UInt16(listenerPort),
                nonce: randomNonce32(),
                lanAddrs: lanAddrs
            )
            let initQr = try initPayload.toQrString()

            // Enable auto-trust on the running node so that when the scanning device
            // connects, we automatically trust them back.
            try? node?.enableQrPairingListener()

            let wc = PairingQRWindowController(
                payload: initQr,
                identityPeerId: myPeerId,
                identityPkB64: myPk,
                identityName: myName,
                lanPort: UInt16(listenerPort),
                onPaired: { [weak self] in
                    try? self?.node?.disableQrPairingListener()
                    self?.updateMenu()
                }
            )
            pairingQRWindowController = wc
            wc.showWindow(nil)
            wc.window?.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
        } catch {
            showError("Failed to generate pairing QR: \(error)")
        }
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
                    nonce: randomNonce32(),
                    lanAddrs: getLanAddresses()
                )
                let initQr = try initPayload.toQrString()

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

                let respQr = PairingHelpers.normalizeQrString(field.stringValue)
                let fin = try PairingHelpers.finalizePairing(initQr: initQr, respQr: respQr)

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

                let initQr = PairingHelpers.normalizeQrString(field.stringValue)
                let initPayload = try pairingPayloadFromQrString(s: initQr)

                let respPayload = pairingPayloadCreate(
                    version: 1,
                    peerId: myPeerId,
                    name: myName,
                    identityPk: Data(base64Encoded: myPk)!.map { $0 },
                    lanPort: UInt16(listenerPort),
                    nonce: initPayload.nonce(),
                    lanAddrs: getLanAddresses()
                )
                let respQr = try respPayload.toQrString()

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
            for i in 0..<bytes.count { bytes[i] = UInt8.random(in: 0...255) }
        }
        return bytes
    }

    // MARK: - Actions

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

    // MARK: - Sync Lifecycle

    private func stopSyncRuntime() {
        syncEnabled = false

        node?.stop()
        node = nil
        handler = nil
        discoveryHandler = nil
        clipboardProvider = nil

        connectedPeers.removeAll()
        nearbyPeers.removeAll()
        updateMenu()
    }

    @objc private func toggleSync() {
        if syncEnabled {
            stopSyncRuntime()
        } else {
            wireUpFFI()
        }
    }

    @objc private func quit() {
        stopSyncRuntime()
        NSApplication.shared.terminate(self)
    }

    // MARK: - Notifications

    private func requestNotificationAuthorizationIfNeeded() {
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            if settings.authorizationStatus != .notDetermined { return }
            center.requestAuthorization(options: [.alert, .sound]) { _, _ in }
        }
    }

    private func postUserNotification(title: String, body: String) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default

        let request = UNNotificationRequest(
            identifier: UUID().uuidString,
            content: content,
            trigger: nil
        )

        UNUserNotificationCenter.current().add(request) { _ in }
    }

    private func showError(_ message: String) {
        postUserNotification(title: "OpenClipboard", body: message)
    }
}
