import AppKit
import CoreImage
import CoreImage.CIFilterBuiltins
import SwiftUI

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
        window.setContentSize(NSSize(width: 460, height: 600))
        window.isReleasedWhenClosed = false
        window.center()

        self.init(window: window)
        self.hostingController = hosting
        self.onPaired = onPaired
    }

    // Legacy convenience for backward compat (shows init-only if needed)
    convenience init(payload: String) {
        self.init(payload: payload, identityPeerId: "", identityPkB64: "", identityName: "", lanPort: 0, onPaired: {})
    }

    func update(payload: String) {
        // no-op for legacy callers
    }
}

// MARK: - Full Pairing Flow View

enum PairingQRStep {
    case showInit
    case waitResponse
    case showCode(code: String, remotePeerId: String, remoteName: String, remotePkB64: String)
    case done(peerId: String)
    case error(String)
}

struct PairingQRFlowView: View {
    let initQr: String
    let myPeerId: String
    let myPkB64: String
    let myName: String
    let lanPort: UInt16
    let onPaired: () -> Void

    @State private var step: PairingQRStep = .showInit
    @State private var responseInput: String = ""
    @State private var copied: Bool = false

    var body: some View {
        VStack(spacing: 16) {
            switch step {
            case .showInit:
                initStepView
            case .waitResponse:
                responseStepView
            case .showCode(let code, let remotePeerId, let remoteName, let remotePkB64):
                confirmStepView(code: code, remotePeerId: remotePeerId, remoteName: remoteName, remotePkB64: remotePkB64)
            case .done(let peerId):
                doneView(peerId: peerId)
            case .error(let msg):
                errorView(msg: msg)
            }
        }
        .padding(20)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: Step 1 — Show Init QR

    private var initStepView: some View {
        VStack(spacing: 12) {
            Text("Step 1: Scan this QR on the other device")
                .font(.headline)

            if let img = qrImage(for: initQr) {
                Image(nsImage: img)
                    .interpolation(.none)
                    .resizable()
                    .frame(width: 240, height: 240)
            }

            HStack {
                Button(copied ? "Copied!" : "Copy Init String") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(initQr, forType: .string)
                    copied = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { copied = false }
                }
                Spacer()
            }

            Divider()

            Button("Next → Enter Response") {
                step = .waitResponse
            }
            .buttonStyle(.borderedProminent)
        }
    }

    // MARK: Step 2 — Paste Response

    private var responseStepView: some View {
        VStack(spacing: 12) {
            Text("Step 2: Paste the response string from the other device")
                .font(.headline)

            TextEditor(text: $responseInput)
                .font(.system(size: 11, design: .monospaced))
                .frame(height: 100)
                .border(Color.gray.opacity(0.3))

            HStack {
                Button("← Back") {
                    step = .showInit
                }

                Spacer()

                Button("Derive Code") {
                    deriveCode()
                }
                .buttonStyle(.borderedProminent)
                .disabled(responseInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
    }

    // MARK: Step 3 — Confirm Code

    private func confirmStepView(code: String, remotePeerId: String, remoteName: String, remotePkB64: String) -> some View {
        VStack(spacing: 16) {
            Text("Step 3: Verify confirmation code")
                .font(.headline)

            Text(code)
                .font(.system(size: 36, weight: .bold, design: .monospaced))
                .padding()
                .background(Color.green.opacity(0.1))
                .cornerRadius(8)

            Text("Confirm this code matches on the other device")
                .foregroundStyle(.secondary)

            Text("Pairing with: \(remoteName) (\(remotePeerId.prefix(8))…)")
                .font(.subheadline)

            HStack {
                Button("Cancel") {
                    step = .showInit
                    responseInput = ""
                }

                Spacer()

                Button("Confirm & Pair") {
                    confirmPairing(remotePeerId: remotePeerId, remoteName: remoteName, remotePkB64: remotePkB64)
                }
                .buttonStyle(.borderedProminent)
            }
        }
    }

    // MARK: Done

    private func doneView(peerId: String) -> some View {
        VStack(spacing: 16) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 48))
                .foregroundColor(.green)
            Text("Paired successfully!")
                .font(.headline)
            Text("Peer: \(peerId)")
                .font(.subheadline)
                .foregroundStyle(.secondary)

            Button("Close") {
                NSApp.keyWindow?.close()
            }
        }
    }

    // MARK: Error

    private func errorView(msg: String) -> some View {
        VStack(spacing: 12) {
            Text("Error")
                .font(.headline)
                .foregroundColor(.red)
            Text(msg)
                .foregroundStyle(.secondary)

            Button("← Back") {
                step = .waitResponse
            }
        }
    }

    // MARK: Logic

    private func deriveCode() {
        do {
            let normalized = PairingHelpers.normalizeQrString(responseInput.trimmingCharacters(in: .whitespacesAndNewlines))
            let fin = try PairingHelpers.finalizePairing(initQr: initQr, respQr: normalized)
            step = .showCode(
                code: fin.code,
                remotePeerId: fin.remotePeerId,
                remoteName: fin.remoteName,
                remotePkB64: fin.remotePkB64
            )
        } catch {
            step = .error("Failed to derive code: \(error.localizedDescription)")
        }
    }

    private func confirmPairing(remotePeerId: String, remoteName: String, remotePkB64: String) {
        do {
            let store = try trustStoreOpen(path: trustStoreDefaultPath())
            try store.add(peerId: remotePeerId, identityPkB64: remotePkB64, displayName: remoteName)
            step = .done(peerId: remotePeerId)
            onPaired()
        } catch {
            step = .error("Failed to save trust: \(error.localizedDescription)")
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
