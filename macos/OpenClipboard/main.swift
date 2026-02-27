import Cocoa
import SwiftUI

@main
struct OpenClipboardApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    
    var body: some Scene {
        Settings {
            SettingsView()
        }
    }
}

class AppDelegate: NSObject, NSApplicationDelegate {
    var statusItem: NSStatusItem?
    var statusBarMenu: NSMenu?
    
    // TODO: Add OpenClipboard FFI integration
    // import OpenClipboard
    // var clipboardNode: ClipboardNode?
    
    func applicationDidFinishLaunching(_ notification: Notification) {
        setupMenuBarApp()
        // TODO: Initialize ClipboardNode
        // clipboardNode = ClipboardNode(identityPath: "...", trustPath: "...")
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
        
        // Peer ID section
        let peerIdItem = NSMenuItem(title: "Peer ID: \(getPeerId())", action: nil, keyEquivalent: "")
        peerIdItem.isEnabled = false
        menu.addItem(peerIdItem)
        
        menu.addItem(NSMenuItem.separator())
        
        // Connected peers section
        menu.addItem(NSMenuItem(title: "Connected Peers", action: nil, keyEquivalent: ""))
        let connectedPeers = getConnectedPeers()
        if connectedPeers.isEmpty {
            let noPeersItem = NSMenuItem(title: "  No peers connected", action: nil, keyEquivalent: "")
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
        
        // Actions
        menu.addItem(NSMenuItem(title: "Send Clipboard", action: #selector(sendClipboard), keyEquivalent: "s"))
        menu.addItem(NSMenuItem(title: "Settings", action: #selector(showSettings), keyEquivalent: ","))
        
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q"))
    }
    
    private func getPeerId() -> String {
        // TODO: Return actual peer ID from ClipboardNode
        // return clipboardNode?.peerId() ?? "unknown"
        return "peer-12345abcdef"
    }
    
    private func getConnectedPeers() -> [String] {
        // TODO: Return actual connected peers
        return ["peer-67890ghijkl", "peer-mnopqr123456"]
    }
    
    @objc private func sendClipboard() {
        // TODO: Implement clipboard sending
        // Get current clipboard content
        // Show peer selection dialog
        // Call clipboardNode.connectAndSendText() or connectAndSendFile()
        
        let alert = NSAlert()
        alert.messageText = "Send Clipboard"
        alert.informativeText = "This feature is not yet implemented. TODO: Integrate with ClipboardNode FFI."
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }
    
    @objc private func showSettings() {
        // Open Settings window
        NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)
    }
    
    @objc private func quit() {
        // TODO: Stop ClipboardNode listener
        // clipboardNode?.stop()
        NSApplication.shared.terminate(self)
    }
}