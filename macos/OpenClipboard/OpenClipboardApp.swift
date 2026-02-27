import Cocoa
import SwiftUI
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

@MainActor
class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem?
    private var statusBarMenu: NSMenu?

    private var node: ClipboardNode?
    private var handler: MacEventHandler?
    private var connectedPeers: [String] = []

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
                try node.startListener(port: listenerPort, handler: handler)
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

        menu.addItem(NSMenuItem(title: "Send Clipboard…", action: #selector(sendClipboard), keyEquivalent: "s"))
        menu.addItem(NSMenuItem(title: "Settings", action: #selector(showSettings), keyEquivalent: ","))

        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q"))
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
