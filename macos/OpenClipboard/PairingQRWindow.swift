import AppKit
import CoreImage
import CoreImage.CIFilterBuiltins
import SwiftUI

@MainActor
final class PairingQRWindowController: NSWindowController {
    private var hostingController: NSHostingController<PairingQRView>?

    convenience init(payload: String) {
        let view = PairingQRView(payload: payload)
        let hosting = NSHostingController(rootView: view)

        let window = NSWindow(contentViewController: hosting)
        window.title = "Pairing QR"
        window.styleMask = [.titled, .closable, .miniaturizable]
        window.setContentSize(NSSize(width: 420, height: 520))
        window.isReleasedWhenClosed = false
        window.center()

        self.init(window: window)
        self.hostingController = hosting
    }

    func update(payload: String) {
        hostingController?.rootView = PairingQRView(payload: payload)
    }
}

struct PairingQRView: View {
    let payload: String

    @State private var copied: Bool = false

    var body: some View {
        VStack(spacing: 12) {
            Text("Show this QR on the other device")
                .font(.headline)

            if let img = qrImage(for: payload) {
                Image(nsImage: img)
                    .interpolation(.none)
                    .resizable()
                    .frame(width: 280, height: 280)
                    .accessibilityLabel("Pairing QR code")
            } else {
                Text("Failed to generate QR")
                    .foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Raw string")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)

                ScrollView {
                    Text(payload)
                        .font(.system(size: 11, design: .monospaced))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(8)
                }
                .frame(height: 120)
                .background(Color(nsColor: .textBackgroundColor))
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
                )

                HStack {
                    Button(copied ? "Copied" : "Copy") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(payload, forType: .string)
                        copied = true
                        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) {
                            copied = false
                        }
                    }

                    Spacer()

                    Text("Tip: If QR scanning isn’t available, copy/paste the string.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .padding(16)
    }

    private func qrImage(for s: String) -> NSImage? {
        let data = Data(s.utf8)
        let filter = CIFilter.qrCodeGenerator()
        filter.setValue(data, forKey: "inputMessage")
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }

        // Scale up with nearest-neighbor.
        let scale: CGFloat = 10
        let transformed = output.transformed(by: CGAffineTransform(scaleX: scale, y: scale))

        let rep = NSCIImageRep(ciImage: transformed)
        let nsImage = NSImage(size: rep.size)
        nsImage.addRepresentation(rep)
        return nsImage
    }
}
