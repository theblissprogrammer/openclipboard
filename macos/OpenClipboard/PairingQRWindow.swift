import AppKit
import CoreImage
import CoreImage.CIFilterBuiltins
import SwiftUI
import OpenClipboardBindings

@MainActor
final class PairingQRWindowController: NSWindowController {
    private var hostingController: NSHostingController<PairingQRFlowView>?
    private var onPaired: (() -> Void)?

    convenience init(payload: String, identityPeerId: String, identityPkB64: String, identityName: String, lanPort: UInt16, onPaired: @escaping () -> Void) {
        let view = PairingQRFlowView(
            initQr: payload,
            myPeerId: identityPeerId,
            myPkB64: identityPkB64,
            myName: identityName,
            lanPort: lanPort,
            onPaired: onPaired
        )
        let hosting = NSHostingController(rootView: view)

        let window = NSWindow(contentViewController: hosting)
        window.title = "Pair Device"
        window.styleMask = [.titled, .closable, .miniaturizable]
        window.setContentSize(NSSize(width: 460, height: 520))
        window.isReleasedWhenClosed = false
        window.center()

        self.init(window: window)
        self.hostingController = hosting
        self.onPaired = onPaired
    }

    // Legacy convenience for backward compat
    convenience init(payload: String) {
        self.init(payload: payload, identityPeerId: "", identityPkB64: "", identityName: "", lanPort: 0, onPaired: {})
    }

    func update(payload: String) {
        // no-op for legacy callers
    }
}

// MARK: - Simplified 1-Step QR Pairing View

struct PairingQRFlowView: View {
    let initQr: String
    let myPeerId: String
    let myPkB64: String
    let myName: String
    let lanPort: UInt16
    let onPaired: () -> Void

    @State private var status: String = "Scan this QR on the other device to pair"
    @State private var copied: Bool = false
    @State private var paired: Bool = false
    @State private var showManualInput: Bool = false
    @State private var manualInput: String = ""
    @State private var error: String? = nil

    var body: some View {
        VStack(spacing: 16) {
            if paired {
                doneView
            } else {
                qrView
            }
        }
        .padding(20)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onAppear {
            enablePairingListener()
        }
        .onDisappear {
            disablePairingListener()
        }
    }

    // MARK: - QR Display

    private var qrView: some View {
        VStack(spacing: 12) {
            Text("Pair Device")
                .font(.headline)

            Text(status)
                .foregroundStyle(.secondary)

            if let img = qrImage(for: initQr) {
                Image(nsImage: img)
                    .interpolation(.none)
                    .resizable()
                    .frame(width: 240, height: 240)
            }

            HStack {
                Button(copied ? "Copied!" : "Copy Pairing String") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(initQr, forType: .string)
                    copied = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { copied = false }
                }
                Spacer()
            }

            if let error = error {
                Text(error)
                    .foregroundColor(.red)
                    .font(.caption)
            }

            Divider()

            // Manual input fallback
            DisclosureGroup("Pair Manually", isExpanded: $showManualInput) {
                VStack(spacing: 8) {
                    Text("Paste a pairing string from the other device:")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextEditor(text: $manualInput)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(height: 60)
                        .border(Color.gray.opacity(0.3))
                    Button("Pair") {
                        pairManually()
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(manualInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
    }

    // MARK: Done

    private var doneView: some View {
        VStack(spacing: 16) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 48))
                .foregroundColor(.green)
            Text("Paired successfully!")
                .font(.headline)

            Button("Close") {
                NSApp.keyWindow?.close()
            }
        }
    }

    // MARK: Logic

    private func enablePairingListener() {
        do {
            let node = try clipboardNodeNew(
                identityPath: defaultIdentityPath(),
                trustPath: trustStoreDefaultPath()
            )
            try node.enableQrPairingListener()
        } catch {
            // Best effort — the main node will handle it
        }
    }

    private func disablePairingListener() {
        do {
            let node = try clipboardNodeNew(
                identityPath: defaultIdentityPath(),
                trustPath: trustStoreDefaultPath()
            )
            try node.disableQrPairingListener()
        } catch {
            // Best effort
        }
    }

    private func pairManually() {
        let input = manualInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !input.isEmpty else { return }

        do {
            let node = try clipboardNodeNew(
                identityPath: defaultIdentityPath(),
                trustPath: trustStoreDefaultPath()
            )
            let peerId = try node.pairViaQr(qrString: input)
            paired = true
            onPaired()
        } catch {
            self.error = "Pairing failed: \(error.localizedDescription)"
        }
    }

    // MARK: QR Generation

    private func qrImage(for s: String) -> NSImage? {
        let data = Data(s.utf8)
        let filter = CIFilter.qrCodeGenerator()
        filter.setValue(data, forKey: "inputMessage")
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let scale: CGFloat = 10
        let transformed = output.transformed(by: CGAffineTransform(scaleX: scale, y: scale))
        let rep = NSCIImageRep(ciImage: transformed)
        let nsImage = NSImage(size: rep.size)
        nsImage.addRepresentation(rep)
        return nsImage
    }
}
